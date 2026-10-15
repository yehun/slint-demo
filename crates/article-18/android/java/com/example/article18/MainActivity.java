package com.example.article18;

import android.app.NativeActivity;
import android.os.Bundle;

import com.example.article18.location.LocationInvoke;

/**
 * 第十八幕: Slint 地图 + 定位 — Android 入口.
 *
 * 必须继承 NativeActivity (Slint 要求), 不能用普通 Activity —— 否则 android_main
 * 不被调用。名字须与 Cargo.toml 里声明的
 * [[package.metadata.android.application.activity]] name = "com.example.article18.MainActivity" 一致。
 */
public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_18");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }

    /** 运行时权限结果 → LocationInvoke(授权后自动启动定位监听). */
    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        LocationInvoke.onPermissionResult(this, requestCode, grantResults);
    }
}
