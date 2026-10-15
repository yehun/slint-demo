package com.example.article18.location;

import android.app.Activity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.os.Looper;
import android.util.Log;

/**
 * 定位能力的 JNI 入口 —— Rust 侧通过 JNI 调用这些静态方法.
 *
 * <p>需要的 Context 由 Rust 传入(就是 NativeActivity 本身). open() 先查运行时
 * 权限: 已授权直接注册监听; 未授权弹系统授权框, 授权结果由 MainActivity 的
 * {@code onRequestPermissionsResult} 转回 {@link #onPermissionResult} 再启动.
 *
 * <p>方法名/参数类型改一个字母都要和 Rust 侧同时改, 否则真机上会报
 * {@code NoSuchMethodError}.
 */
public class LocationInvoke {

    private static final String TAG = "LocationInvoke";

    /** 运行时权限请求码 */
    public static final int PERMISSION_REQUEST_CODE = 4101;

    private static final long INTERVAL_MS = 1000;

    /** 当前监听器(open 注册, close 注销) */
    private static LocationListener listener;

    private LocationInvoke() {
    }

    /** 定位开关(GPS/网络/被动任一 provider 可用即视为可用) */
    public static boolean enabled(Context context) {
        LocationManager lm =
                (LocationManager) context.getSystemService(Context.LOCATION_SERVICE);
        if (lm == null) {
            return false;
        }
        String[] providers = new String[]{
                LocationManager.GPS_PROVIDER,
                LocationManager.NETWORK_PROVIDER,
                LocationManager.PASSIVE_PROVIDER
        };
        for (String provider : providers) {
            try {
                if (lm.isProviderEnabled(provider)) {
                    return true;
                }
            } catch (Exception ignore) {
            }
        }
        return lm.isLocationEnabled();
    }

    /** 开启定位: 权限已授 → 注册监听; 未授 → 弹系统授权框(结果走 onPermissionResult). */
    public static void open(Context context) {
        Activity activity = (Activity) context;
        if (activity.checkSelfPermission(android.Manifest.permission.ACCESS_FINE_LOCATION)
                == PackageManager.PERMISSION_GRANTED) {
            start(activity);
            return;
        }
        Log.w(TAG, "定位权限未授权, 请求运行时权限");
        activity.requestPermissions(
                new String[]{
                        android.Manifest.permission.ACCESS_FINE_LOCATION,
                        android.Manifest.permission.ACCESS_COARSE_LOCATION
                },
                PERMISSION_REQUEST_CODE);
    }

    /** 权限结果回填(由 MainActivity.onRequestPermissionsResult 调用). */
    public static void onPermissionResult(Context context, int requestCode, int[] results) {
        if (requestCode != PERMISSION_REQUEST_CODE) {
            return;
        }
        for (int result : results) {
            if (result == PackageManager.PERMISSION_GRANTED) {
                Log.i(TAG, "定位权限已授权, 启动定位");
                start(context);
                return;
            }
        }
        Log.w(TAG, "定位权限被拒, 不启动定位");
    }

    /** 注册系统定位监听(GPS/网络/被动三个 provider 全注册, 谁有结果用谁). */
    private static void start(Context context) {
        LocationManager lm =
                (LocationManager) context.getSystemService(Context.LOCATION_SERVICE);
        if (lm == null) {
            return;
        }
        if (listener != null) {
            // 幂等: 重复 open 先注销旧监听
            lm.removeUpdates(listener);
        }
        listener = new LocationListener() {
            @Override
            public void onLocationChanged(Location location) {
                // 经 JNI static native 回到 Rust:
                // Java_com_example_article18_location_LocationCallback_onLocationChanged
                LocationCallback.onLocationChanged(
                        location.getProvider(),
                        location.getLatitude(),
                        location.getLongitude(),
                        location.getAccuracy());
            }
        };
        String[] providers = new String[]{
                LocationManager.GPS_PROVIDER,
                LocationManager.NETWORK_PROVIDER,
                LocationManager.PASSIVE_PROVIDER
        };
        for (String provider : providers) {
            try {
                if (lm.isProviderEnabled(provider)) {
                    // 回调派发到主线程 Looper(有 Looper 的线程才能收)
                    lm.requestLocationUpdates(
                            provider, INTERVAL_MS, 0f, listener, context.getMainLooper());
                    Log.i(TAG, "requestLocationUpdates " + provider + " ok");
                }
            } catch (Exception e) {
                Log.w(TAG, "requestLocationUpdates " + provider + " failed: " + e);
            }
        }
    }

    /** 停止定位并注销监听. */
    public static void close(Context context) {
        if (listener == null) {
            return;
        }
        LocationManager lm =
                (LocationManager) context.getSystemService(Context.LOCATION_SERVICE);
        if (lm != null) {
            lm.removeUpdates(listener);
        }
        listener = null;
        Log.i(TAG, "定位已停止");
    }
}
