package com.osvauld.android;

import android.app.Activity;
import android.graphics.Insets;
import android.os.Build;
import android.view.View;
import android.view.WindowInsets;

public final class InsetsAdapter {
    private InsetsAdapter() {}
    private static native void update(long token, int left, int top, int right, int bottom);

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
