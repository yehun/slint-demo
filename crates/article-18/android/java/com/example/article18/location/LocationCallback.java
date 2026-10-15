package com.example.article18.location;

/**
 * 定位回调 —— Java 定位线程 → Rust 的 native 桥.
 *
 * <p>native 实现在 Rust 侧 location/android.rs
 * (导出符号 {@code Java_com_example_article18_location_LocationCallback_onLocationChanged},
 * 类路径改了要两边同步).
 */
public class LocationCallback {

    /**
     * 定位变化
     *
     * @param provider 位置提供者("gps" / "network" / "passive")
     * @param latitude 纬度(WGS-84, GPS/系统定位标准输出)
     * @param longitude 经度
     * @param accuracy 精度: 米
     */
    public static native void onLocationChanged(
            String provider,
            double latitude,
            double longitude,
            float accuracy);
}
