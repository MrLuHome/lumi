// Objective-C 桥接：只暴露一个“输入 RGBA8 → 输出 JSON”的函数。
// 不加载任何外部权重，不上传图像，使用系统 Vision 框架做本地静态照片分析。

#import <Foundation/Foundation.h>
#import <Vision/Vision.h>
#import <CoreGraphics/CoreGraphics.h>
#import <string.h>

static char *lumi_copy_string(NSString *value) {
    const char *utf8 = [value UTF8String];
    if (!utf8) {
        return NULL;
    }
    size_t length = strlen(utf8) + 1;
    char *copy = (char *)malloc(length);
    if (copy) {
        memcpy(copy, utf8, length);
    }
    return copy;
}

static void lumi_set_error(char **error, NSString *message) {
    if (error) {
        *error = lumi_copy_string(message);
    }
}

static NSArray *lumi_region_points(VNFaceLandmarkRegion2D *region,
                                   CGSize imageSize,
                                   float confidence) {
    if (!region || region.pointCount == 0) {
        return @[];
    }
    // ObjC 版 Vision 只提供 `pointsInImageOfSize:`（CGPoint 数组），
    // 没有逐点置信度；这里使用人脸级 confidence，并在文档中如实标注。
    const CGPoint *points = [region pointsInImageOfSize:imageSize];
    if (!points) {
        return @[];
    }
    NSUInteger count = region.pointCount;
    NSMutableArray *result = [NSMutableArray arrayWithCapacity:count];
    for (NSUInteger index = 0; index < count; index++) {
        CGPoint point = points[index];
        // 该接口返回左下角原点的像素坐标，这里转成左上角归一化坐标。
        double x = point.x / imageSize.width;
        double y = 1.0 - point.y / imageSize.height;
        [result addObject:@{@"x": @(x), @"y": @(y), @"confidence": @(confidence)}];
    }
    return result;
}

/// 返回值为堆分配的 UTF-8 JSON，调用方用 `lumi_vision_free` 释放。
/// 失败时返回 NULL，并在 `error` 中写入原因。
char *lumi_vision_analyze(const uint8_t *rgba,
                          int width,
                          int height,
                          char **error) {
    if (error) {
        *error = NULL;
    }
    if (!rgba || width <= 0 || height <= 0) {
        lumi_set_error(error, @"图像缓冲区为空或尺寸非法");
        return NULL;
    }
    @autoreleasepool {
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        if (!colorSpace) {
            lumi_set_error(error, @"无法创建 RGB 色彩空间");
            return NULL;
        }
        CFDataRef data = CFDataCreate(NULL, rgba, (CFIndex)width * height * 4);
        CGDataProviderRef provider = CGDataProviderCreateWithCFData(data);
        CGImageRef image = CGImageCreate((size_t)width,
                                         (size_t)height,
                                         8,
                                         32,
                                         (size_t)width * 4,
                                         colorSpace,
                                         kCGImageAlphaNoneSkipLast | kCGBitmapByteOrderDefault,
                                         provider,
                                         NULL,
                                         false,
                                         kCGRenderingIntentDefault);
        CGColorSpaceRelease(colorSpace);
        CGDataProviderRelease(provider);
        CFRelease(data);
        if (!image) {
            lumi_set_error(error, @"无法从像素创建 CGImage");
            return NULL;
        }

        VNDetectFaceLandmarksRequest *request = [[VNDetectFaceLandmarksRequest alloc] init];
        VNImageRequestHandler *handler =
            [[VNImageRequestHandler alloc] initWithCGImage:image options:@{}];
        NSError *visionError = nil;
        BOOL ok = [handler performRequests:@[request] error:&visionError];
        CGImageRelease(image);
        if (!ok) {
            lumi_set_error(error,
                           [NSString stringWithFormat:@"Vision 请求失败：%@",
                                                      visionError.localizedDescription ?: @"未知错误"]);
            return NULL;
        }

        NSArray<VNFaceObservation *> *observations = request.results ?: @[];
        NSMutableArray *faces = [NSMutableArray arrayWithCapacity:observations.count];
        CGSize imageSize = CGSizeMake(width, height);
        for (VNFaceObservation *observation in observations) {
            CGRect bbox = observation.boundingBox;
            NSMutableDictionary *face = [NSMutableDictionary dictionary];
            face[@"x"] = @(bbox.origin.x);
            face[@"y"] = @(1.0 - (bbox.origin.y + bbox.size.height));
            face[@"width"] = @(bbox.size.width);
            face[@"height"] = @(bbox.size.height);
            face[@"confidence"] = @(observation.confidence);

            NSMutableDictionary *regions = [NSMutableDictionary dictionary];
            VNFaceLandmarks2D *landmarks = observation.landmarks;
            if (landmarks) {
                float confidence = (float)observation.confidence;
                void (^add_region)(NSString *, VNFaceLandmarkRegion2D *) =
                    ^(NSString *key, VNFaceLandmarkRegion2D *region) {
                        if (!region) {
                            return;
                        }
                        NSArray *points = lumi_region_points(region, imageSize, confidence);
                        if (points.count > 0) {
                            regions[key] = points;
                        }
                    };
                add_region(@"faceContour", landmarks.faceContour);
                add_region(@"leftEye", landmarks.leftEye);
                add_region(@"rightEye", landmarks.rightEye);
                add_region(@"leftEyebrow", landmarks.leftEyebrow);
                add_region(@"rightEyebrow", landmarks.rightEyebrow);
                add_region(@"nose", landmarks.nose);
                add_region(@"noseCrest", landmarks.noseCrest);
                add_region(@"medianLine", landmarks.medianLine);
                add_region(@"outerLips", landmarks.outerLips);
                add_region(@"innerLips", landmarks.innerLips);
            }
            face[@"regions"] = regions;
            [faces addObject:face];
        }

        NSDictionary *payload = @{
            @"faces": faces,
            @"revision": @(request.revision),
        };
        NSData *json = [NSJSONSerialization dataWithJSONObject:payload options:0 error:&visionError];
        if (!json) {
            lumi_set_error(error,
                           [NSString stringWithFormat:@"结果序列化失败：%@",
                                                      visionError.localizedDescription ?: @"未知错误"]);
            return NULL;
        }
        NSString *text = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
        return lumi_copy_string(text);
    }
}

void lumi_vision_free(char *pointer) {
    if (pointer) {
        free(pointer);
    }
}
