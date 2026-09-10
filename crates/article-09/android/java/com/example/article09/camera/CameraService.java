package com.example.article09.camera;

import android.content.Context;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;

/**
 * 相机服务编排层.
 *
 * 职责:
 * 1. 管理后台线程 (CameraBackground)
 * 2. 协调权限/配置/图像三个子服务
 * 3. 提供 startCamera/stopCamera/toggleCamera 三个操作
 */
public class CameraService {
    private static final String TAG = "CameraService";

    private final Context context;
    private HandlerThread backgroundThread;
    private Handler backgroundHandler;
    private CameraConfigManager configManager;
    private CameraPermissionService permissionService;
    private boolean isFrontCamera = false;

    public CameraService(Context context) {
        this.context = context;
        this.configManager = new CameraConfigManager(context);
        this.permissionService = new CameraPermissionService(context);
    }

    public void setOnFrameCallback(CameraCallbacks.OnFrameCallback callback) {
        configManager.setOnFrameCallback(callback);
    }

    public void setOnStateCallback(CameraCallbacks.OnStateCallback callback) {
        configManager.setOnStateCallback(callback);
    }

    public void setOnPermissionCallback(CameraCallbacks.OnPermissionCallback callback) {
        permissionService.setOnPermissionCallback(callback);
    }

    public void startCamera() {
        Log.d(TAG, "startCamera() 开始, 权限=" + permissionService.hasCameraPermission());
        if (!permissionService.hasCameraPermission()) {
            Log.d(TAG, "无权限, 发起权限申请");
            permissionService.requestCameraPermission();
            return;
        }
        Log.d(TAG, "有权限, 启动后台线程和相机");
        startBackgroundThread();
        configManager.startCamera(backgroundHandler, isFrontCamera, false);
    }

    public void toggleCamera() {
        isFrontCamera = !isFrontCamera;
        configManager.closeCamera(false);
        startBackgroundThread();
        configManager.startCamera(backgroundHandler, isFrontCamera, false);
    }

    public void stopCamera() {
        configManager.closeCamera(false);
        stopBackgroundThread();
    }

    public void onPermissionResult(boolean granted) {
        permissionService.onPermissionResult(granted);
    }

    private void startBackgroundThread() {
        if (backgroundThread != null && backgroundThread.isAlive()) return;
        backgroundThread = new HandlerThread("CameraBackground");
        backgroundThread.start();
        backgroundHandler = new Handler(backgroundThread.getLooper());
    }

    private void stopBackgroundThread() {
        if (backgroundThread == null) return;
        try {
            backgroundThread.quitSafely();
            backgroundThread.join(100);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } finally {
            backgroundThread = null;
            backgroundHandler = null;
        }
    }
}
