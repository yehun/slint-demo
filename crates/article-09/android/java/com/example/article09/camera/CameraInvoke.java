package com.example.article09.camera;

import android.util.Log;
import com.example.article09.MainActivity;

/**
 * JNI 调用: Rust → Java.
 *
 * Rust 通过 JNI 调用这些静态方法, 控制 Java 侧的相机服务.
 */
public class CameraInvoke {
    private static final String TAG = "CameraInvoke";
    public static void open() {
        Log.d(TAG, "open() 被调用");
        CameraService svc = MainActivity.getCameraService();
        if (svc == null) { Log.e(TAG, "CameraService 为 null!"); return; }
        svc.startCamera();
    }
    public static void toggle() {
        Log.d(TAG, "toggle() 被调用");
        CameraService svc = MainActivity.getCameraService();
        if (svc == null) { Log.e(TAG, "CameraService 为 null!"); return; }
        svc.toggleCamera();
    }
    public static void close() {
        Log.d(TAG, "close() 被调用");
        CameraService svc = MainActivity.getCameraService();
        if (svc == null) { Log.e(TAG, "CameraService 为 null!"); return; }
        svc.stopCamera();
    }
}
