package com.example.article09.camera;

import android.media.Image;
import android.util.Log;

/**
 * YUV_420_888 → RGB 转换 + 旋转 + 镜像.
 *
 * 性能关键路径:
 * - 整数运算代替浮点 (BT.601 公式)
 * - BufferPool 复用字节数组
 * - ThreadLocal 避免多线程竞争
 */
public class ImageConversion {
    private static final String TAG = "ImageConversion";
    public static final int BYTES_PER_RGB_PIX = 3;

    private static final BufferPool bufferPool = BufferPool.getInstance();

    // 线程本地存储, 避免多线程竞争
    private static final ThreadLocal<ConversionContext> threadLocalContext =
            new ThreadLocal<ConversionContext>() {
                @Override
                protected ConversionContext initialValue() {
                    return new ConversionContext();
                }
            };

    private static class ConversionContext {
        byte[] yRow;
        byte[] uRow;
        byte[] vRow;
        byte[] rgbRow;

        void ensureSize(int ySize, int uSize, int vSize, int finalSize) {
            if (yRow == null || yRow.length < ySize) yRow = bufferPool.getByteArray(ySize);
            if (uRow == null || uRow.length < uSize) uRow = bufferPool.getByteArray(uSize);
            if (vRow == null || vRow.length < vSize) vRow = bufferPool.getByteArray(vSize);
            if (rgbRow == null || rgbRow.length < finalSize) rgbRow = bufferPool.getByteArray(finalSize);
        }
    }

    private static int clamp(int value) {
        return value < 0 ? 0 : (value > 255 ? 255 : value);
    }

    /**
     * 将 YUV_420_888 格式的 Image 转换为 RGB 字节数组.
     */
    public static byte[] convertToRGB(Image yuvImage) {
        if (yuvImage == null) return null;

        int width = yuvImage.getWidth();
        int height = yuvImage.getHeight();
        int bufferSize = BYTES_PER_RGB_PIX * width * height;
        byte[] rgbData = bufferPool.getByteArray(bufferSize);

        try {
            Image.Plane[] planes = yuvImage.getPlanes();
            if (planes.length < 3) return null;

            Image.Plane yPlane = planes[0];
            Image.Plane uPlane = planes[1];
            Image.Plane vPlane = planes[2];

            int yRowStride = yPlane.getRowStride();
            int uRowStride = uPlane.getRowStride();
            int vRowStride = vPlane.getRowStride();
            int yPixelStride = yPlane.getPixelStride();
            int uPixelStride = uPlane.getPixelStride();
            int vPixelStride = vPlane.getPixelStride();

            byte[] yBuffer = new byte[yPlane.getBuffer().remaining()];
            byte[] uBuffer = new byte[uPlane.getBuffer().remaining()];
            byte[] vBuffer = new byte[vPlane.getBuffer().remaining()];
            yPlane.getBuffer().get(yBuffer);
            uPlane.getBuffer().get(uBuffer);
            vPlane.getBuffer().get(vBuffer);

            ConversionContext ctx = threadLocalContext.get();
            ctx.ensureSize(yRowStride, uRowStride, vRowStride, width * BYTES_PER_RGB_PIX);

            for (int row = 0; row < height; row++) {
                int yOffset = row * yRowStride;
                int uvOffset = (row / 2) * uRowStride;
                int uvVOffset = (row / 2) * vRowStride;

                for (int col = 0; col < width; col++) {
                    int y = yBuffer[yOffset + col * yPixelStride] & 0xFF;
                    int u = uBuffer[uvOffset + (col / 2) * uPixelStride] & 0xFF;
                    int v = vBuffer[uvVOffset + (col / 2) * vPixelStride] & 0xFF;

                    // BT.601 整数运算
                    int c = y - 16, d = u - 128, e = v - 128;
                    int r = clamp((298 * c + 409 * e + 128) >> 8);
                    int g = clamp((298 * c - 100 * d - 208 * e + 128) >> 8);
                    int b = clamp((298 * c + 516 * d + 128) >> 8);

                    int rgbIndex = (row * width + col) * BYTES_PER_RGB_PIX;
                    rgbData[rgbIndex] = (byte) r;
                    rgbData[rgbIndex + 1] = (byte) g;
                    rgbData[rgbIndex + 2] = (byte) b;
                }
            }
            return rgbData;
        } catch (Exception e) {
            Log.e(TAG, "YUV→RGB 转换错误", e);
            bufferPool.returnByteArray(rgbData);
            return null;
        }
    }

    /** 90度顺时针旋转 */
    public static void rotate90Into(byte[] input, int width, int height, byte[] output) {
        int newWidth = height;
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int srcIdx = (y * width + x) * BYTES_PER_RGB_PIX;
                int dstIdx = (x * newWidth + (newWidth - 1 - y)) * BYTES_PER_RGB_PIX;
                if (srcIdx + 2 < input.length && dstIdx + 2 < output.length) {
                    output[dstIdx] = input[srcIdx];
                    output[dstIdx + 1] = input[srcIdx + 1];
                    output[dstIdx + 2] = input[srcIdx + 2];
                }
            }
        }
    }

    /** 180度旋转 */
    public static void rotate180Into(byte[] input, int width, int height, byte[] output) {
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int srcIdx = (y * width + x) * BYTES_PER_RGB_PIX;
                int dstIdx = ((height - 1 - y) * width + (width - 1 - x)) * BYTES_PER_RGB_PIX;
                if (srcIdx + 2 < input.length && dstIdx + 2 < output.length) {
                    output[dstIdx] = input[srcIdx];
                    output[dstIdx + 1] = input[srcIdx + 1];
                    output[dstIdx + 2] = input[srcIdx + 2];
                }
            }
        }
    }

    /** 270度顺时针旋转 */
    public static void rotate270Into(byte[] input, int width, int height, byte[] output) {
        int newWidth = height;
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int srcIdx = (y * width + x) * BYTES_PER_RGB_PIX;
                int dstIdx = ((width - 1 - x) * newWidth + y) * BYTES_PER_RGB_PIX;
                if (srcIdx + 2 < input.length && dstIdx + 2 < output.length) {
                    output[dstIdx] = input[srcIdx];
                    output[dstIdx + 1] = input[srcIdx + 1];
                    output[dstIdx + 2] = input[srcIdx + 2];
                }
            }
        }
    }

    /** 水平镜像 (原地操作) */
    public static void mirrorHorizontalRGBInPlace(byte[] buffer, int width, int height) {
        int rowStride = width * BYTES_PER_RGB_PIX;
        byte[] temp = new byte[BYTES_PER_RGB_PIX];
        for (int y = 0; y < height; y++) {
            int rowStart = y * rowStride;
            for (int x = 0; x < width / 2; x++) {
                int left = rowStart + x * BYTES_PER_RGB_PIX;
                int right = rowStart + (width - 1 - x) * BYTES_PER_RGB_PIX;
                System.arraycopy(buffer, left, temp, 0, BYTES_PER_RGB_PIX);
                System.arraycopy(buffer, right, buffer, left, BYTES_PER_RGB_PIX);
                System.arraycopy(temp, 0, buffer, right, BYTES_PER_RGB_PIX);
            }
        }
    }

    public static void safeReturnByteArray(byte[] array) {
        bufferPool.returnByteArray(array);
    }

    public static void cleanup() {
        threadLocalContext.remove();
        bufferPool.clear();
    }
}
