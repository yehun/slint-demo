package com.example.article09;

import android.Manifest;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.app.NativeActivity;
import android.util.Log;

import com.example.article09.camera.CameraCallbacks;
import com.example.article09.camera.CameraService;
import com.example.article09.camera.CameraCallback;

/**
 * 第九幕: Slint 读取摄像头 — Android 入口.
 *
 * 职责:
 * 1. 继承 NativeActivity (Slint 要求)
 * 2. 加载 native 库 (libarticle09.so)
 * 3. 创建 CameraService, 注册帧/状态/权限回调
 * 4. 连接 Java ↔ Rust 双向 JNI 通路
 */
public class MainActivity extends NativeActivity {
    private static final String TAG = "MainActivity";
    private static final int CAMERA_PERMISSION_REQUEST = 1001;

    static {
        System.loadLibrary("article_09");
        Log.d(TAG, "Native library loaded");
    }

    private static MainActivity instance;
    public static MainActivity getInstance() { return instance; }

    private static CameraService cameraService;
    public static CameraService getCameraService() { return cameraService; }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
        initCamera();
    }

    private void initCamera() {
        Log.d(TAG, "initCamera() 开始");
        cameraService = new CameraService(this);
        Log.d(TAG, "CameraService 创建完成");

        // 权限回调
        cameraService.setOnPermissionCallback(new CameraCallbacks.OnPermissionCallback() {
            @Override public void onPermissionRequired() {
                requestPermissions(new String[]{Manifest.permission.CAMERA}, CAMERA_PERMISSION_REQUEST);
            }
            @Override public void onPermissionGranted() {
                cameraService.startCamera();
            }
            @Override public void onPermissionDenied() {
                CameraCallback.onCameraState(-1); // 通知 Rust 出错
            }
        });

        // 帧回调: 直接转发给 Rust 的 native 方法
        cameraService.setOnFrameCallback(new CameraCallbacks.OnFrameCallback() {
            @Override public void onFrame(byte[] data, int width, int height) {
                CameraCallback.onCameraFrame(data, width, height);
            }
        });

        // 状态回调: 1=打开, 0=关闭, -1=错误
        cameraService.setOnStateCallback(new CameraCallbacks.OnStateCallback() {
            @Override public void onOpened() { CameraCallback.onCameraState(1); }
            @Override public void onClosed() { CameraCallback.onCameraState(0); }
            @Override public void onError(int error) { CameraCallback.onCameraState(-1); }
        });
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == CAMERA_PERMISSION_REQUEST) {
            boolean granted = grantResults.length > 0 && grantResults[0] == PackageManager.PERMISSION_GRANTED;
            cameraService.onPermissionResult(granted);
        }
    }

    @Override
    protected void onPause() {
        super.onPause();
        if (cameraService != null) cameraService.stopCamera();
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (cameraService != null) cameraService.stopCamera();
        if (instance != null) instance = null;
    }
}
