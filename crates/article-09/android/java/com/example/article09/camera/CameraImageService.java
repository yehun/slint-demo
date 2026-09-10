package com.example.article09.camera;

import android.media.Image;
import android.util.Log;

/**
 * 帧图像处理服务.
 *
 * 流程: YUV→RGB 转换 → 旋转 → 前置相机镜像 → 回调通知.
 */
public class CameraImageService {
    private static final String TAG = "CameraImageService";
    private static final long MIN_FRAME_INTERVAL = 33; // ~30 FPS

    private long lastProcessTime = 0;
    private boolean isFrontCamera = false;
    private byte[] transformBuffer;
    private int transformBufferSize;
    private CameraCallbacks.OnFrameCallback onFrameCallback;

    public void setOnFrameCallback(CameraCallbacks.OnFrameCallback callback) {
        this.onFrameCallback = callback;
    }

    public void setFrontCamera(boolean front) {
        this.isFrontCamera = front;
    }

    /**
     * 处理一帧图像.
     *
     * @param image             YUV_420_888 格式的图像
     * @param displayRotation   屏幕旋转角度 (Surface.ROTATION_0/90/180/270)
     * @param sensorOrientation 相机传感器安装角度 (0/90/180/270, 大多数手机为 90)
     */
    public void processImage(Image image, int displayRotation, int sensorOrientation) {
        long now = System.currentTimeMillis();
        if (now - lastProcessTime < MIN_FRAME_INTERVAL) return;

        try {
            int width = image.getWidth();
            int height = image.getHeight();

            // 1. YUV → RGB
            byte[] rgbData = ImageConversion.convertToRGB(image);
            if (rgbData == null) return;

            // 2. 计算旋转角度
            int rotation;
            if (isFrontCamera) {
                rotation = (sensorOrientation + displayRotation) % 360;
            } else {
                rotation = (sensorOrientation - displayRotation + 360) % 360;
            }

            // 3. 执行旋转 (如果需要)
            byte[] processedData;
            int finalWidth = width;
            int finalHeight = height;
            if (rotation == 0) {
                processedData = rgbData;
            } else if (rotation == 90) {
                finalWidth = height;
                finalHeight = width;
                processedData = new byte[finalWidth * finalHeight * 3];
                ImageConversion.rotate90Into(rgbData, width, height, processedData);
            } else if (rotation == 180) {
                processedData = new byte[width * height * 3];
                ImageConversion.rotate180Into(rgbData, width, height, processedData);
            } else { // rotation == 270
                finalWidth = height;
                finalHeight = width;
                processedData = new byte[finalWidth * finalHeight * 3];
                ImageConversion.rotate270Into(rgbData, width, height, processedData);
            }

            // 4. 前置相机水平镜像
            if (isFrontCamera) {
                ImageConversion.mirrorHorizontalRGBInPlace(processedData, finalWidth, finalHeight);
            }

            // 5. 回调通知
            if (onFrameCallback != null) {
                onFrameCallback.onFrame(processedData, finalWidth, finalHeight);
            }

            lastProcessTime = now;
        } catch (Exception e) {
            Log.e(TAG, "帧处理错误", e);
        }
    }

    public void cleanup() {
        if (transformBuffer != null) {
            ImageConversion.safeReturnByteArray(transformBuffer);
            transformBuffer = null;
        }
        lastProcessTime = 0;
    }
}
