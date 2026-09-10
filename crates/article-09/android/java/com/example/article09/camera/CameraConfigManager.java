package com.example.article09.camera;

import android.content.Context;
import android.graphics.ImageFormat;
import android.hardware.camera2.*;
import android.hardware.camera2.params.StreamConfigurationMap;
import android.media.Image;
import android.media.ImageReader;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;
import android.util.Size;

import java.util.Arrays;
import java.util.Comparator;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Camera2 API 底层封装.
 *
 * 流程:
 * 1. 枚举摄像头, 通过 LENS_FACING 区分前后摄
 * 2. 选择最接近目标分辨率 (1280x720) 的输出尺寸
 * 3. 创建 ImageReader (YUV_420_888) + 帧回调
 * 4. openCamera → createCaptureSession → setRepeatingRequest
 */
public class CameraConfigManager {
    private static final String TAG = "CameraConfigManager";
    private static final int MAX_BUFFERS = 3;
    private static final int PREFERRED_WIDTH = 1280;
    private static final int PREFERRED_HEIGHT = 720;

    private final Context context;
    private final CameraManager cameraManager;
    private CameraDevice cameraDevice;
    private CameraCaptureSession cameraSession;
    private ImageReader imageReader;
    private CameraImageService imageService;
    private String frontCameraId;
    private String backCameraId;

    private CameraCallbacks.OnStateCallback onStateCallback;
    private final AtomicBoolean isCameraRunning = new AtomicBoolean(false);
    private final AtomicBoolean isStarting = new AtomicBoolean(false);
    private int sensorOrientation;
    private Size previewSize;

    public CameraConfigManager(Context context) {
        this.context = context;
        this.cameraManager = (CameraManager) context.getSystemService(Context.CAMERA_SERVICE);
        this.imageService = new CameraImageService();
        initializeCameraIds();
    }

    public void setOnStateCallback(CameraCallbacks.OnStateCallback callback) {
        this.onStateCallback = callback;
    }

    public void setOnFrameCallback(CameraCallbacks.OnFrameCallback callback) {
        this.imageService.setOnFrameCallback(callback);
    }

    private void initializeCameraIds() {
        try {
            for (String id : cameraManager.getCameraIdList()) {
                CameraCharacteristics chars = cameraManager.getCameraCharacteristics(id);
                Integer facing = chars.get(CameraCharacteristics.LENS_FACING);
                if (facing != null) {
                    if (facing == CameraCharacteristics.LENS_FACING_BACK) backCameraId = id;
                    else if (facing == CameraCharacteristics.LENS_FACING_FRONT) frontCameraId = id;
                }
            }
        } catch (Exception e) {
            Log.e(TAG, "初始化摄像头 ID 失败", e);
        }
    }

    private Size chooseOptimalSize(String cameraId) {
        try {
            CameraCharacteristics chars = cameraManager.getCameraCharacteristics(cameraId);
            StreamConfigurationMap map = chars.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
            if (map == null) return new Size(PREFERRED_WIDTH, PREFERRED_HEIGHT);
            Size[] sizes = map.getOutputSizes(ImageFormat.YUV_420_888);
            if (sizes == null || sizes.length == 0) return new Size(PREFERRED_WIDTH, PREFERRED_HEIGHT);

            Size optimal = sizes[0];
            int targetArea = PREFERRED_WIDTH * PREFERRED_HEIGHT;
            int minDiff = Integer.MAX_VALUE;
            for (Size size : sizes) {
                int diff = Math.abs(size.getWidth() * size.getHeight() - targetArea);
                if (diff < minDiff) { minDiff = diff; optimal = size; }
            }
            return optimal;
        } catch (Exception e) {
            return new Size(PREFERRED_WIDTH, PREFERRED_HEIGHT);
        }
    }

    private String getCameraId(boolean useFront) {
        return useFront
                ? (frontCameraId != null ? frontCameraId : backCameraId)
                : (backCameraId != null ? backCameraId : frontCameraId);
    }

    public synchronized void startCamera(Handler backgroundHandler, boolean isFront, boolean toggled) {
        if (isCameraRunning.get() || isStarting.get()) return;
        isStarting.set(true);

        try {
            String cameraId = getCameraId(isFront);
            if (cameraId == null) { callbackError(-1); isStarting.set(false); return; }

            imageService.setFrontCamera(isFront);
            previewSize = chooseOptimalSize(cameraId);
            sensorOrientation = getSensorOrientation(cameraId);

            imageReader = ImageReader.newInstance(
                    previewSize.getWidth(), previewSize.getHeight(),
                    ImageFormat.YUV_420_888, MAX_BUFFERS);
            imageReader.setOnImageAvailableListener(reader -> {
                try (Image image = reader.acquireLatestImage()) {
                    if (image != null) {
                        int displayRotation = getDisplayRotation();
                        imageService.processImage(image, displayRotation, sensorOrientation);
                    }
                } catch (Exception e) {
                    Log.e(TAG, "帧处理错误", e);
                }
            }, backgroundHandler);

            cameraManager.openCamera(cameraId, new CameraDevice.StateCallback() {
                @Override public void onOpened(CameraDevice camera) {
                    cameraDevice = camera;
                    isCameraRunning.set(true);
                    isStarting.set(false);
                    createCameraPreview(backgroundHandler);
                    if (!toggled) callbackOpened();
                }
                @Override public void onDisconnected(CameraDevice camera) {
                    callbackClosed();
                    closeCamera(false);
                }
                @Override public void onError(CameraDevice camera, int error) {
                    callbackError(error);
                    isStarting.set(false);
                    closeCamera(false);
                }
            }, backgroundHandler);
        } catch (Exception e) {
            Log.e(TAG, "启动相机失败", e);
            callbackError(-4);
            isStarting.set(false);
        }
    }

    private void createCameraPreview(Handler backgroundHandler) {
        try {
            CaptureRequest.Builder builder =
                    cameraDevice.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW);
            builder.addTarget(imageReader.getSurface());
            builder.set(CaptureRequest.CONTROL_AF_MODE,
                    CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE);

            cameraDevice.createCaptureSession(
                    Arrays.asList(imageReader.getSurface()),
                    new CameraCaptureSession.StateCallback() {
                        @Override public void onConfigured(CameraCaptureSession session) {
                            cameraSession = session;
                            try {
                                session.setRepeatingRequest(builder.build(), null, backgroundHandler);
                            } catch (CameraAccessException e) {
                                callbackError(-5);
                            }
                        }
                        @Override public void onConfigureFailed(CameraCaptureSession session) {
                            callbackError(-6);
                        }
                    }, backgroundHandler);
        } catch (Exception e) {
            callbackError(-7);
        }
    }

    private int getSensorOrientation(String cameraId) {
        try {
            return cameraManager.getCameraCharacteristics(cameraId)
                    .get(CameraCharacteristics.SENSOR_ORIENTATION);
        } catch (Exception e) {
            return 90;
        }
    }

    /**
     * 获取屏幕旋转角度 (Surface.ROTATION_0/90/180/270).
     * 用于计算帧旋转: 大多数手机 sensorOrientation=90, 竖屏 displayRotation=0,
     * 后摄 rotation = (90 - 0 + 360) % 360 = 90 (需要顺时针旋转 90 度).
     */
    private int getDisplayRotation() {
        try {
            android.view.WindowManager wm = (android.view.WindowManager)
                    context.getSystemService(android.content.Context.WINDOW_SERVICE);
            if (wm == null) return 0;
            switch (wm.getDefaultDisplay().getRotation()) {
                case android.view.Surface.ROTATION_90:  return 90;
                case android.view.Surface.ROTATION_180: return 180;
                case android.view.Surface.ROTATION_270: return 270;
                default: return 0;
            }
        } catch (Exception e) {
            return 0;
        }
    }

    public synchronized void closeCamera(boolean toggled) {
        if (cameraSession != null) {
            try { cameraSession.stopRepeating(); cameraSession.abortCaptures(); cameraSession.close(); }
            catch (Exception e) { Log.e(TAG, "关闭 session 失败", e); }
            cameraSession = null;
        }
        if (cameraDevice != null) {
            try { cameraDevice.close(); } catch (Exception e) { }
            cameraDevice = null;
        }
        if (imageReader != null) {
            try { imageReader.close(); } catch (Exception e) { }
            imageReader = null;
        }
        isCameraRunning.set(false);
        isStarting.set(false);
        imageService.cleanup();
        if (!toggled) callbackClosed();
    }

    private void callbackOpened()  { if (onStateCallback != null) onStateCallback.onOpened(); }
    private void callbackClosed() { if (onStateCallback != null) onStateCallback.onClosed(); }
    private void callbackError(int e) { if (onStateCallback != null) onStateCallback.onError(e); }
}
