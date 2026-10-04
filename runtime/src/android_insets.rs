//! Android UI-thread inset updates. Per-installation tokens are invalidated on suspend/drop;
//! stale callbacks cannot publish into the resumed runner. Callbacks never access app/render state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak, atomic::{AtomicI64, Ordering}};
use jni::{JNIEnv, JavaVM, NativeMethod, objects::{JClass, JObject, JValue}};
use winit::platform::android::activity::{AndroidApp, AndroidAppWaker};

struct State {
    insets: Mutex<Option<[i32; 4]>>,
    waker: AndroidAppWaker,
}

fn registry() -> &'static Mutex<HashMap<i64, Weak<State>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<i64, Weak<State>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) struct Adapter {
    token: i64,
    state: Arc<State>,
    activity: AndroidApp,
}

impl Adapter {
    pub fn install(activity: &AndroidApp) -> Result<Self, String> {
        static NEXT: AtomicI64 = AtomicI64::new(1);
        let adapter = Self {
            token: NEXT.fetch_add(1, Ordering::Relaxed),
            activity: activity.clone(),
            state: Arc::new(State { insets: Mutex::new(None), waker: activity.create_waker() }),
        };
        registry().lock().map_err(|e| e.to_string())?
            .insert(adapter.token, Arc::downgrade(&adapter.state));
        // AndroidApp owns these VM/activity pointers throughout this call. Make a local activity
        // reference inside a bounded JNI frame; do not delete AndroidApp's global reference.
        let vm = unsafe { JavaVM::from_raw(activity.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let result = env.with_local_frame(16, |env| -> jni::errors::Result<()> {
            let activity = env.new_local_ref(unsafe {
                JObject::from_raw(activity.activity_as_ptr().cast())
            })?;
            let loader = env.call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
            let name = env.new_string("com.osvauld.android.InsetsAdapter")?;
            let class = JClass::from(env.call_method(&loader, "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(name.as_ref())])?.l()?);
            env.register_native_methods(&class, &[NativeMethod {
                name: "update".into(), sig: "(JIIII)V".into(), fn_ptr: update as *mut std::ffi::c_void,
            }])?;
            env.call_static_method(&class, "install", "(Landroid/app/Activity;J)V",
                &[JValue::Object(&activity), JValue::Long(adapter.token)])?;
            Ok(())
        });
        if let Err(error) = result {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
            return Err(format!("inset listener setup: {error}"));
        }
        Ok(adapter)
    }

    pub fn keyboard(&self, visible: bool) {
        // Keep Winit's show policy; blur must also hide a keyboard opened by the user.
        if visible { self.activity.show_soft_input(true); }
        else { self.activity.hide_soft_input(false); }
    }

    pub fn latest(&self) -> Option<[i32; 4]> {
        self.state.insets.lock().ok().and_then(|value| *value)
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        if let Ok(mut entries) = registry().lock() {
            entries.remove(&self.token);
        }
    }
}

extern "system" fn update(_: JNIEnv<'_>, _: JClass<'_>, token: i64, l: i32, t: i32, r: i32, b: i32) {
    let insets = [l, t, r, b];
    if insets.iter().any(|v| *v < 0) { return; }
    let state = registry().lock().ok().and_then(|entries| entries.get(&token)?.upgrade());
    let Some(state) = state else { return };
    let changed = if let Ok(mut latest) = state.insets.lock() {
        let changed = *latest != Some(insets);
        *latest = Some(insets);
        changed
    } else { false };
    if changed {
        #[cfg(debug_assertions)]
        eprintln!("ANDROID_INSETS token={token} safe_px={insets:?}");
        state.waker.wake();
    }
}
