package com.example.article15;

import android.app.NativeActivity;
import android.util.Log;

public class MainActivity extends NativeActivity {
    private static final String TAG = "Article15";

    static {
        System.loadLibrary("article_15");
    }
}
