package com.example.article09.camera;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * 相机权限管理.
 */
public class CameraPermissionService {
    private final Context context;
    private final AtomicBoolean hasCameraPermission = new AtomicBoolean(false);
    private CameraCallbacks.OnPermissionCallback onPermissionCallback;

    public CameraPermissionService(Context context) {
        this.context = context;
        checkCameraPermission();
    }

    public void setOnPermissionCallback(CameraCallbacks.OnPermissionCallback callback) {
        this.onPermissionCallback = callback;
    }

    public boolean checkCameraPermission() {
        boolean granted = context.checkSelfPermission(Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED;
        hasCameraPermission.set(granted);
        return granted;
    }

    public void onPermissionResult(boolean granted) {
        hasCameraPermission.set(granted);
        if (onPermissionCallback != null) {
            if (granted) onPermissionCallback.onPermissionGranted();
            else onPermissionCallback.onPermissionDenied();
        }
    }

    public void requestCameraPermission() {
        if (onPermissionCallback != null) {
            onPermissionCallback.onPermissionRequired();
        }
    }

    public boolean hasCameraPermission() {
        return hasCameraPermission.get();
    }
}
