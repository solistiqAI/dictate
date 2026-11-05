//! The JNI side of `app.dictate.Native` (android/app/src/main/java/app/dictate/Native.kt): one
//! loaded model per process; errors and panics come out as `IllegalStateException` with the
//! message. The Kotlin side calls this from a background thread, never from the main thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};

use anyhow::{Context, Result, anyhow};
use jni::JNIEnv;
use jni::objects::{JClass, JFloatArray, JObjectArray, JString};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jobjectArray, jstring};

use crate::{Dictation, Files};

type Job = Box<dyn FnOnce(&mut Option<Dictation>) + Send>;

/// Runs `job` on the thread that owns the model (the engines are not `Send`), one at a time, and
/// waits for its result.
fn with_dictation<T: Send + 'static>(
    job: impl FnOnce(&mut Option<Dictation>) -> T + Send + 'static,
) -> Result<T> {
    static WORKER: OnceLock<Sender<Job>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (sender, jobs) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("dictate-asr".into())
            .stack_size(8 << 20)
            .spawn(move || {
                let mut dictation = None;
                for job in jobs {
                    // A panicking job drops its reply (the caller hears of it), the thread lives on.
                    let _ = catch_unwind(AssertUnwindSafe(|| job(&mut dictation)));
                }
            })
            .expect("start the speech thread");
        sender
    });
    let (reply, answer) = mpsc::sync_channel(1);
    worker
        .send(Box::new(move |dictation| {
            let _ = reply.send(job(dictation));
        }))
        .map_err(|_| anyhow!("the speech thread is gone"))?;
    answer
        .recv()
        .map_err(|_| anyhow!("internal error in the speech thread"))
}

/// Runs `body`; an error or a panic becomes a Java exception and `fallback` the return value.
fn guard<'l, T>(
    env: &mut JNIEnv<'l>,
    fallback: T,
    body: impl FnOnce(&mut JNIEnv<'l>) -> Result<T>,
) -> T {
    let result = catch_unwind(AssertUnwindSafe(|| body(env))).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        Err(anyhow!("internal error: {message}"))
    });
    match result {
        Ok(value) => value,
        Err(e) => {
            // An exception may be pending already (a failed JNI call); keep that one.
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new("java/lang/IllegalStateException", format!("{e:#}"));
            }
            fallback
        }
    }
}

fn string(env: &mut JNIEnv, value: &JString) -> Result<String> {
    Ok(env.get_string(value).context("read a string")?.into())
}

fn strings(env: &mut JNIEnv, array: &JObjectArray) -> Result<Vec<String>> {
    let len = env.get_array_length(array)?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let item = JString::from(env.get_object_array_element(array, i)?);
        out.push(string(env, &item)?);
        env.delete_local_ref(item)?;
    }
    Ok(out)
}

fn string_array(env: &mut JNIEnv, items: &[String]) -> Result<jobjectArray> {
    let array = env.new_object_array(items.len() as i32, "java/lang/String", JString::default())?;
    for (i, item) in items.iter().enumerate() {
        let value = env.new_string(item)?;
        env.set_object_array_element(&array, i as i32, &value)?;
        env.delete_local_ref(value)?;
    }
    Ok(array.into_raw())
}

/// `load(modelDir, dataDir)`: loads the model (seconds) and the settings; replaces one loaded
/// before.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_load<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    model_dir: JString<'l>,
    data_dir: JString<'l>,
) {
    guard(&mut env, (), |env| {
        let model_dir = PathBuf::from(string(env, &model_dir)?);
        let files = Files::new(string(env, &data_dir)?);
        with_dictation(move |slot| {
            *slot = None; // free the old model before the new one takes its memory
            *slot = Some(Dictation::load(&model_dir, files)?);
            Ok(())
        })?
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_isLoaded<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jboolean {
    match with_dictation(|slot| slot.is_some()) {
        Ok(true) => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

/// `transcribe(samples, sampleRate)`: mono floats in [-1, 1] to the finished text.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_transcribe<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    samples: JFloatArray<'l>,
    rate: jint,
) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let len = env.get_array_length(&samples)? as usize;
        let mut buffer = vec![0.0f32; len];
        env.get_float_array_region(&samples, 0, &mut buffer)?;
        let rate = u32::try_from(rate).context("negative sample rate")?;
        let text = with_dictation(move |slot| {
            slot.as_mut()
                .context("no model loaded")?
                .transcribe(buffer, rate)
        })??;
        Ok(env.new_string(text)?.into_raw())
    })
}

/// `reloadSettings()`: after the dictionary changed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_reloadSettings<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
) {
    let _ = with_dictation(|slot| {
        if let Some(dictation) = slot {
            dictation.reload_settings();
        }
    });
}

/// `dictionary(dataDir)`: the whole word list.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_dictionary<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    data_dir: JString<'l>,
) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let files = Files::new(string(env, &data_dir)?);
        string_array(env, &crate::dictionary(&files))
    })
}

/// `setDictionary(dataDir, words)`: saves the whole word list.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_setDictionary<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    data_dir: JString<'l>,
    words: JObjectArray<'l>,
) {
    guard(&mut env, (), |env| {
        let files = Files::new(string(env, &data_dir)?);
        let words = strings(env, &words)?;
        crate::set_dictionary(&files, &words)
    })
}

/// `modelName()`: the folder name below `models/`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_modelName<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        Ok(env.new_string(crate::MODEL)?.into_raw())
    })
}

/// `modelFiles()`: URL and file name, alternating.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_modelFiles<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let flat: Vec<String> = crate::model_files()
            .into_iter()
            .flat_map(|(url, name)| [url, name])
            .collect();
        string_array(env, &flat)
    })
}

/// `modelInstalled(modelDir)`: all files are there.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_dictate_Native_modelInstalled<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    model_dir: JString<'l>,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let dir = PathBuf::from(string(env, &model_dir)?);
        Ok(match crate::model_installed(&dir) {
            true => JNI_TRUE,
            false => JNI_FALSE,
        })
    })
}
