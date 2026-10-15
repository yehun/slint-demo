package com.example.article19;

import android.app.NativeActivity;
import android.os.Bundle;
import android.content.pm.PackageManager;
import android.Manifest;

/**
 * 第十九幕: 噪音测量 — Android 入口.
 *
 * 必须继承 NativeActivity (Slint 要求), 不能用普通 Activity —— 否则 android_main
 * 不被调用。名字须与 Cargo.toml 里声明的
 * [[package.metadata.android.application.activity]] name = "com.example.article19.MainActivity" 一致。
 *
 * 麦克风是危险权限, 在 onCreate 里主动申请 RECORD_AUDIO; 授权后用户点「开始测量」
 * 时 cpal(oboe 后端)才能打开输入流。
 */
public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_19");
    }

    private static final int REQ_RECORD_AUDIO = 1001;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.RECORD_AUDIO}, REQ_RECORD_AUDIO);
        }
    }
}
