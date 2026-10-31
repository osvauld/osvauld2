package com.osvauld.android;

import android.app.Activity;
import android.graphics.Insets;
import android.os.Build;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;

public final class InsetsAdapter {
    private InsetsAdapter() {}
    private static native void update(long token, int left, int top, int right, int bottom);

    public static void fullscreen(Activity activity, boolean enabled) {
        activity.runOnUiThread(() -> {
            View decor = activity.getWindow().getDecorView();
            if (Build.VERSION.SDK_INT >= 30) {
                WindowInsetsController controller = decor.getWindowInsetsController();
                if (controller != null) {
                    controller.setSystemBarsBehavior(
                            WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                    if (enabled) controller.hide(WindowInsets.Type.systemBars());
                    else controller.show(WindowInsets.Type.systemBars());
                }
            } else {
                decor.setSystemUiVisibility(enabled
                        ? View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                        : View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
            }
            decor.requestApplyInsets();
        });
    }

    public static void install(Activity activity, long token) {
        activity.runOnUiThread(() -> {
            View decor = activity.getWindow().getDecorView();
            decor.setOnApplyWindowInsetsListener((view, insets) -> {
                if (Build.VERSION.SDK_INT >= 30) {
                    // IME includes the navigation area; union the types, do not add their heights.
                    Insets safe = insets.getInsets(WindowInsets.Type.systemBars()
                            | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
                    update(token, safe.left, safe.top, safe.right, safe.bottom);
                } else {
                    update(token, insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                            insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
                }
                return insets;
            });
            decor.requestApplyInsets();
        });
    }
}
