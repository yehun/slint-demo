package com.example.article14;

import android.app.NativeActivity;
import android.content.Intent;
import android.util.Log;

public class MainActivity extends NativeActivity {
    private static final String TAG = "Article14";

    static {
        System.loadLibrary("article_14");
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        Log.d(TAG, "onActivityResult: requestCode=" + requestCode + " resultCode=" + resultCode);
        super.onActivityResult(requestCode, resultCode, data);
        com.yehun.slintfs.SlintFilePicker.handle(requestCode, resultCode, data);
    }
}
