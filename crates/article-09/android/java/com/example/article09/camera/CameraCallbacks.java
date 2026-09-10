package com.example.article09.camera;

public interface CameraCallbacks {

    interface OnFrameCallback {
        void onFrame(byte[] data, int width, int height);
    }

    interface OnStateCallback {
        void onOpened();
        void onClosed();
        void onError(int error);
    }

    interface OnPermissionCallback {
        void onPermissionRequired();
        void onPermissionGranted();
        void onPermissionDenied();
    }
}
