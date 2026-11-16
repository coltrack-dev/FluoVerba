//! Speech scheduling is kept outside the ticker UI and storage layer.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
        Arc,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechSettings {
    pub enabled: bool,
    pub russian_enabled: bool,
    pub language: String,
    pub rate: f32,
    pub russian_volume: f32,
    pub russian_voice: String,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            russian_enabled: true,
            language: "es-ES".into(),
            rate: 1.0,
            russian_volume: 0.75,
            russian_voice: String::new(),
        }
    }
}

impl SpeechSettings {
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "enabled" => settings.enabled = value == "true",
                    "russian_enabled" => settings.russian_enabled = value == "true",
                    "russian_voice" => settings.russian_voice = value.into(),
                    "language" if ["es-ES", "es-CO", "es-MX"].contains(&value) => {
                        settings.language = value.into()
                    }
                    "rate" => {
                        if let Ok(rate) = value.parse::<f32>() {
                            if rate.is_finite() {
                                settings.rate = rate.clamp(0.5, 1.5);
                            }
                        }
                    }
                    "russian_volume" => {
                        if let Ok(volume) = value.parse::<f32>() {
                            if volume.is_finite() {
                                settings.russian_volume = volume.clamp(0.3, 1.0);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        settings
    }

    pub fn serialize(&self) -> String {
        format!(
            "enabled={}\nrussian_enabled={}\nlanguage={}\nrate={}\nrussian_volume={}\nrussian_voice={}\n",
            self.enabled,
            self.russian_enabled,
            self.language,
            self.rate,
            self.russian_volume,
            self.russian_voice
        )
    }
}

#[derive(Debug, PartialEq)]
enum QueueAction {
    Stop,
    Speak {
        spanish: String,
        russian: String,
        utterance: u64,
        replace: bool,
    },
}

#[derive(Default)]
struct SpeechQueue {
    enabled: bool,
    generation: u64,
    last_phrase_id: Option<(i64, i64)>,
    russian_enabled: bool,
}

impl SpeechQueue {
    fn set_enabled(&mut self, enabled: bool) -> Option<QueueAction> {
        let was_enabled = self.enabled;
        self.enabled = enabled;
        if was_enabled && !enabled {
            Some(QueueAction::Stop)
        } else {
            None
        }
    }

    fn phrase_changed(
        &mut self,
        phrase_id: (i64, i64),
        spanish: &str,
        russian: &str,
    ) -> Option<QueueAction> {
        if self.last_phrase_id == Some(phrase_id) {
            return None;
        }
        self.last_phrase_id = Some(phrase_id);
        self.generation = self.generation.wrapping_add(1);
        if self.enabled {
            Some(QueueAction::Speak {
                spanish: spanish.into(),
                russian: if self.russian_enabled {
                    russian.into()
                } else {
                    String::new()
                },
                utterance: self.generation,
                replace: true,
            })
        } else {
            None
        }
    }

    #[cfg(test)]
    fn test_voice(&mut self) -> QueueAction {
        self.generation = self.generation.wrapping_add(1);
        QueueAction::Speak {
            spanish: TEST_VOICE_PHRASE.into(),
            russian: String::new(),
            utterance: self.generation,
            replace: true,
        }
    }
}

#[cfg(test)]
const TEST_VOICE_PHRASE: &str = "Hola. Esta es una prueba de voz de FluoVerba.";

enum Command {
    Enabled(bool),
    Rate(f32),
    Language(String),
    Phrase((i64, i64), String, String, u64),
    RussianEnabled(bool),
    RussianVolume(f32),
    RussianVoice(String),
    RussianVoices(Box<dyn FnOnce(Vec<String>, String) + Send>),
    TestRussianVoice,
    TestVoice,
    Persist(PathBuf, SpeechSettings),
    Shutdown,
}

/// A non-blocking handle used by the Slint thread. Android owns a system TTS bridge;
/// desktop builds keep the same API and intentionally provide a no-op fallback.
pub struct SpeechService {
    sender: Sender<Command>,
    generation: Arc<AtomicU64>,
}

impl SpeechService {
    #[cfg(not(target_os = "android"))]
    pub fn desktop(settings: &SpeechSettings) -> Self {
        let (sender, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let initial = settings.clone();
        let _ = std::thread::Builder::new()
            .name("fluoverba-speech-fallback".into())
            .spawn(move || {
                let mut queue = SpeechQueue::default();
                queue.set_enabled(initial.enabled);
                queue.russian_enabled = initial.russian_enabled;
                while let Ok(command) = receiver.recv() {
                    match command {
                        Command::Enabled(value) => {
                            queue.set_enabled(value);
                        }
                        Command::Phrase(phrase_id, spanish, russian, generation) => {
                            let _ = generation;
                            let _ = queue.phrase_changed(phrase_id, &spanish, &russian);
                        }
                        Command::TestVoice => {}
                        Command::TestRussianVoice => {}
                        Command::RussianEnabled(value) => queue.russian_enabled = value,
                        Command::RussianVolume(volume) => {
                            let _ = volume;
                        }
                        Command::RussianVoice(voice) => {
                            let _ = voice;
                        }
                        Command::RussianVoices(callback) => callback(Vec::new(), String::new()),
                        Command::Persist(path, settings) => persist_settings(&path, &settings),
                        Command::Rate(rate) => {
                            let _ = rate;
                        }
                        Command::Language(language) => {
                            let _ = language;
                        }
                        Command::Shutdown => break,
                    }
                }
            });
        Self { sender, generation }
    }

    #[cfg(target_os = "android")]
    pub fn android(app: slint::android::AndroidApp, settings: &SpeechSettings) -> Self {
        let (sender, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current_generation = generation.clone();
        let initial = settings.clone();
        let _ = std::thread::Builder::new()
            .name("fluoverba-speech".into())
            .spawn(move || {
                android_worker(app, receiver, initial, current_generation);
            });
        Self { sender, generation }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.sender.send(Command::Enabled(enabled));
    }
    pub fn set_rate(&self, rate: f32) {
        let _ = self.sender.send(Command::Rate(rate.clamp(0.5, 1.5)));
    }
    pub fn set_language(&self, language: String) {
        let _ = self.sender.send(Command::Language(language));
    }
    pub fn set_russian_enabled(&self, enabled: bool) {
        let _ = self.sender.send(Command::RussianEnabled(enabled));
    }
    pub fn set_russian_volume(&self, volume: f32) {
        let _ = self
            .sender
            .send(Command::RussianVolume(volume.clamp(0.3, 1.0)));
    }
    pub fn set_russian_voice(&self, voice: String) {
        let _ = self.sender.send(Command::RussianVoice(voice));
    }
    pub fn on_russian_voices(&self, callback: impl FnOnce(Vec<String>, String) + Send + 'static) {
        let _ = self.sender.send(Command::RussianVoices(Box::new(callback)));
    }
    pub fn phrase_changed_with_id(&self, phrase_id: (i64, i64), spanish: &str, russian: &str) {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.sender.send(Command::Phrase(
            phrase_id,
            limit_tts_text(spanish),
            limit_tts_text(russian),
            generation,
        ));
    }

    pub fn test_voice(&self) {
        let _ = self.sender.send(Command::TestVoice);
    }
    pub fn test_russian_voice(&self) {
        let _ = self.sender.send(Command::TestRussianVoice);
    }

    pub fn persist(&self, path: PathBuf, settings: SpeechSettings) {
        let _ = self.sender.send(Command::Persist(path, settings));
    }
}

impl Clone for SpeechService {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            generation: self.generation.clone(),
        }
    }
}

impl SpeechService {
    pub fn shutdown(&self) {
        let _ = self.sender.send(Command::Shutdown);
    }
}

fn persist_settings(path: &PathBuf, settings: &SpeechSettings) {
    let temporary = path.with_extension("ticker-settings.tmp");
    #[cfg(target_os = "windows")]
    if path.exists() {
        if let Err(error) = std::fs::remove_file(path) {
            eprintln!("Could not replace speech settings: {error}");
            return;
        }
    }
    if let Err(error) = std::fs::write(&temporary, settings.serialize())
        .and_then(|()| std::fs::rename(&temporary, path))
    {
        eprintln!("Could not save speech settings: {error}");
        let _ = std::fs::remove_file(temporary);
    }
}

#[cfg(target_os = "android")]
fn android_worker(
    app: slint::android::AndroidApp,
    receiver: mpsc::Receiver<Command>,
    settings: SpeechSettings,
    latest_generation: Arc<AtomicU64>,
) {
    use jni::{
        objects::{JObject, JValue},
        refs::Global,
        JavaVM,
    };

    let vm = match JavaVM::singleton() {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!("TTS Java VM unavailable: {error}");
            return;
        }
    };
    android_log(
        &vm,
        "i",
        "speech worker started; initializing Android TextToSpeech",
    );
    let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
    android_log(&vm, "i", "JNI call SpeechBridge constructor");
    let bridge: Global<JObject<'static>> = match vm.attach_current_thread(|env| {
        // AndroidApp owns this global Activity reference for the worker's lifetime.
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity_ptr)? };
        // This worker is attached from native code, so FindClass uses the bootstrap
        // loader and cannot see classes packaged in the APK. Resolve through the
        // Activity's application class loader instead.
        let class_loader = env
            .call_method(
                activity.as_ref(),
                jni::jni_str!("getClassLoader"),
                jni::jni_sig!("()Ljava/lang/ClassLoader;"),
                &[],
            )?
            .l()?;
        let class_name = env.new_string("org.fluoverba.SpeechBridge")?;
        let class = env
            .call_method(
                &class_loader,
                jni::jni_str!("loadClass"),
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                &[JValue::Object(&class_name)],
            )?
            .l()?;
        let class = env.cast_local::<jni::objects::JClass>(class)?;
        let object = env.new_object(
            class,
            jni::jni_sig!("(Landroid/content/Context;)V"),
            &[JValue::Object(activity.as_ref())],
        )?;
        let ready = env
            .call_method(
                &object,
                jni::jni_str!("awaitInitialization"),
                jni::jni_sig!("()Z"),
                &[],
            )?
            .z()?;
        if !ready {
            let _ = env.call_method(&object, jni::jni_str!("close"), jni::jni_sig!("()V"), &[]);
            return Err(jni::errors::Error::JavaException);
        }
        env.new_global_ref(object)
    }) {
        Ok(bridge) => bridge,
        Err(error) => {
            android_log(&vm, "e", &format!("SpeechBridge startup failed: {error}"));
            eprintln!("Android TextToSpeech initialization failed: {error}");
            return;
        }
    };

    let mut queue = SpeechQueue::default();
    queue.set_enabled(settings.enabled);
    queue.russian_enabled = settings.russian_enabled;
    let mut requested_language = settings.language.clone();
    let actual = call_string(&vm, &bridge, "setLanguage", &requested_language).unwrap_or_default();
    if actual.is_empty() {
        android_log(
            &vm,
            "e",
            &format!("no offline Spanish voice selected for {requested_language}"),
        );
    }
    let _ = call_rate(&vm, &bridge, settings.rate);
    let _ = call_bool(&vm, &bridge, "setRussianEnabled", settings.russian_enabled);
    let _ = call_float(&vm, &bridge, "setRussianVolume", settings.russian_volume);
    let voices_text = call_string_zero(&vm, &bridge, "getRussianVoices").unwrap_or_default();
    let russian_voices: Vec<String> = voices_text
        .lines()
        .filter(|voice| !voice.is_empty())
        .map(str::to_owned)
        .collect();
    for voice in &russian_voices {
        android_log(&vm, "i", &format!("available local ru-RU voice={voice}"));
    }
    let selected_russian_voice =
        call_string(&vm, &bridge, "setRussianVoice", &settings.russian_voice).unwrap_or_default();
    if russian_voices.is_empty() {
        android_log(
            &vm,
            "e",
            "ERROR: no installed offline ru-RU voice is available; install local Russian voice data in Android settings.",
        );
    }

    while let Ok(command) = receiver.recv() {
        match command {
            Command::Enabled(value) => {
                android_log(&vm, "i", &format!("toggle enabled={value}"));
                if let Some(QueueAction::Stop) = queue.set_enabled(value) {
                    let _ = call_void(&vm, &bridge, "stop", "()V", &[]);
                }
            }
            Command::Rate(rate) => {
                android_log(&vm, "i", &format!("rate changed={rate}"));
                let _ = call_rate(&vm, &bridge, rate);
            }
            Command::Language(language) => {
                android_log(&vm, "i", &format!("language changed={language}"));
                requested_language = language;
                let actual = call_string(&vm, &bridge, "setLanguage", &requested_language)
                    .unwrap_or_default();
                if actual.is_empty() {
                    eprintln!("No available Spanish TTS voice for {}", requested_language);
                }
            }
            Command::Phrase(phrase_id, spanish, russian, generation) => {
                if latest_generation.load(Ordering::Relaxed) != generation {
                    continue;
                }
                android_log(
                    &vm,
                    "i",
                    &format!("phrase transition id={phrase_id:?} generation={generation}"),
                );
                if let Some(QueueAction::Speak {
                    spanish,
                    russian,
                    utterance,
                    replace,
                }) = queue.phrase_changed(phrase_id, &spanish, &russian)
                {
                    let pair_id = format!("{}:{}:{utterance}", phrase_id.0, phrase_id.1);
                    let _ = call_speak_pair(&vm, &bridge, &spanish, &russian, &pair_id, replace);
                }
            }
            Command::TestVoice => {
                android_log(&vm, "i", "test voice button request accepted");
                let _ = call_test_voice(&vm, &bridge);
            }
            Command::TestRussianVoice => {
                android_log(&vm, "i", "test Russian voice button request accepted");
                let _ = call_test_russian_voice(&vm, &bridge);
            }
            Command::RussianEnabled(enabled) => {
                queue.russian_enabled = enabled;
                android_log(&vm, "i", &format!("Russian translation enabled={enabled}"));
                let _ = call_bool(&vm, &bridge, "setRussianEnabled", enabled);
            }
            Command::RussianVolume(volume) => {
                let _ = call_float(&vm, &bridge, "setRussianVolume", volume);
            }
            Command::RussianVoice(voice) => {
                let actual =
                    call_string(&vm, &bridge, "setRussianVoice", &voice).unwrap_or_default();
                android_log(
                    &vm,
                    if actual.is_empty() { "e" } else { "i" },
                    &format!("Russian voice selected={actual}"),
                );
            }
            Command::RussianVoices(callback) => {
                callback(russian_voices.clone(), selected_russian_voice.clone());
            }
            Command::Persist(path, settings) => persist_settings(&path, &settings),
            Command::Shutdown => break,
        }
    }
    android_log(&vm, "i", "speech worker shutting down");
    let _ = call_void(&vm, &bridge, "close", "()V", &[]);
}

#[cfg(target_os = "android")]
fn android_log(vm: &jni::JavaVM, priority: &str, message: &str) {
    use jni::{jni_sig, jni_str, objects::JValue};
    let _ = vm
        .attach_current_thread(|env| -> jni::errors::Result<()> {
            let log = env.find_class(jni_str!("android/util/Log"))?;
            let tag = env.new_string("FLUOVERBA_TTS")?;
            let message = env.new_string(message)?;
            let method = match priority {
                "e" => jni_str!("e"),
                "w" => jni_str!("w"),
                _ => jni_str!("i"),
            };
            env.call_static_method(
                log,
                method,
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)I"),
                &[JValue::Object(&tag), JValue::Object(&message)],
            )?;
            Ok(())
        })
        .map_err(|error| eprintln!("[FLUOVERBA_TTS] Android log failed: {error}"));
}

#[cfg(target_os = "android")]
fn call_speak_pair(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    spanish: &str,
    russian: &str,
    pair_id: &str,
    replace: bool,
) -> jni::errors::Result<i32> {
    use jni::{jni_sig, jni_str, objects::JValue};
    android_log(
        vm,
        "i",
        &format!("JNI call speakPair id={pair_id} replace={replace}"),
    );
    let result = vm.attach_current_thread(|env| {
        let java_spanish = env.new_string(spanish)?;
        let java_russian = env.new_string(russian)?;
        let java_id = env.new_string(pair_id)?;
        Ok(env
            .call_method(
                bridge.as_ref(),
                jni_str!("speakPair"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)I"),
                &[
                    JValue::Object(&java_spanish),
                    JValue::Object(&java_russian),
                    JValue::Object(&java_id),
                    JValue::Bool(replace.into()),
                ],
            )?
            .i()?)
    });
    match result {
        Ok(code) => {
            android_log(
                vm,
                if code < 0 { "e" } else { "i" },
                &format!("speakPair Spanish return code={code}"),
            );
            Ok(code)
        }
        Err(error) => {
            android_log(vm, "e", &format!("JNI speakPair failed: {error}"));
            Err(error)
        }
    }
}

#[cfg(target_os = "android")]
fn call_test_voice(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
) -> jni::errors::Result<()> {
    android_log(vm, "i", "JNI call testVoice");
    let result = vm.attach_current_thread(|env| {
        let result = env
            .call_method(
                bridge.as_ref(),
                jni::jni_str!("testVoice"),
                jni::jni_sig!("()I"),
                &[],
            )?
            .i()?;
        Ok(result)
    });
    match result {
        Ok(code) => {
            android_log(
                vm,
                if code < 0 { "e" } else { "i" },
                &format!("testVoice speak return code={code}"),
            );
            Ok(())
        }
        Err(error) => {
            android_log(vm, "e", &format!("JNI testVoice failed: {error}"));
            Err(error)
        }
    }
}

#[cfg(target_os = "android")]
fn call_test_russian_voice(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
) -> jni::errors::Result<()> {
    android_log(vm, "i", "JNI call testRussianVoice");
    let result = vm.attach_current_thread(|env| {
        Ok(env
            .call_method(
                bridge.as_ref(),
                jni::jni_str!("testRussianVoice"),
                jni::jni_sig!("()I"),
                &[],
            )?
            .i()?)
    });
    match result {
        Ok(code) => {
            android_log(
                vm,
                if code < 0 { "e" } else { "i" },
                &format!("testRussianVoice speak return code={code}"),
            );
            Ok(())
        }
        Err(error) => {
            android_log(vm, "e", &format!("JNI testRussianVoice failed: {error}"));
            Err(error)
        }
    }
}

#[cfg(target_os = "android")]
fn call_bool(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    method: &str,
    value: bool,
) -> jni::errors::Result<()> {
    android_log(vm, "i", &format!("JNI call {method}={value}"));
    let method = jni::strings::JNIString::from(method);
    let result = vm.attach_current_thread(|env| {
        env.call_method(
            bridge.as_ref(),
            &method,
            jni::jni_sig!("(Z)V"),
            &[jni::objects::JValue::Bool(value.into())],
        )?;
        Ok(())
    });
    if let Err(error) = &result {
        android_log(
            vm,
            "e",
            &format!("JNI boolean setting call failed: {error}"),
        );
    }
    result
}

#[cfg(target_os = "android")]
fn call_float(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    method: &str,
    value: f32,
) -> jni::errors::Result<()> {
    android_log(vm, "i", &format!("JNI call {method}={value}"));
    let method = jni::strings::JNIString::from(method);
    let result = vm.attach_current_thread(|env| {
        env.call_method(
            bridge.as_ref(),
            &method,
            jni::jni_sig!("(F)V"),
            &[jni::objects::JValue::Float(value)],
        )?;
        Ok(())
    });
    if let Err(error) = &result {
        android_log(vm, "e", &format!("JNI float setting call failed: {error}"));
    }
    result
}

#[cfg(target_os = "android")]
fn call_string_zero(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    method: &str,
) -> jni::errors::Result<String> {
    android_log(vm, "i", &format!("JNI call {method}"));
    let method = jni::strings::JNIString::from(method);
    let result = vm.attach_current_thread(|env| {
        let object = env
            .call_method(
                bridge.as_ref(),
                &method,
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let string = jni::objects::JString::cast_local(env, object)?;
        Ok(string.to_string())
    });
    if let Err(error) = &result {
        android_log(vm, "e", &format!("JNI string query failed: {error}"));
    }
    result
}

/// Android TextToSpeech accepts at most 4,000 UTF-16 code units per request.
/// Clip on Unicode scalar boundaries before marshalling the string through JNI.
fn limit_tts_text(text: &str) -> String {
    let mut utf16_units = 0;
    text.chars()
        .take_while(|character| {
            let next = utf16_units + character.len_utf16();
            if next <= 4_000 {
                utf16_units = next;
                true
            } else {
                false
            }
        })
        .collect()
}

#[cfg(target_os = "android")]
fn call_void(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    method: &str,
    signature: &str,
    args: &[jni::objects::JValue],
) -> jni::errors::Result<()> {
    android_log(vm, "i", &format!("JNI call {method}"));
    let method = jni::strings::JNIString::from(method);
    let signature = jni::signature::RuntimeMethodSignature::from_str(signature)?;
    let result = vm.attach_current_thread(|env| {
        env.call_method(bridge.as_ref(), &method, signature.method_signature(), args)?;
        Ok(())
    });
    if let Err(error) = &result {
        android_log(vm, "e", &format!("JNI call failed: {error}"));
    }
    result
}

#[cfg(target_os = "android")]
fn call_rate(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    rate: f32,
) -> jni::errors::Result<()> {
    android_log(vm, "i", &format!("JNI call setRate {rate}"));
    let result = vm.attach_current_thread(|env| {
        env.call_method(
            bridge.as_ref(),
            jni::jni_str!("setRate"),
            jni::jni_sig!("(F)V"),
            &[jni::objects::JValue::Float(rate)],
        )?;
        Ok(())
    });
    if let Err(error) = &result {
        android_log(vm, "e", &format!("JNI setRate failed: {error}"));
    }
    result
}

#[cfg(target_os = "android")]
fn call_string(
    vm: &jni::JavaVM,
    bridge: &jni::refs::Global<jni::objects::JObject<'static>>,
    method: &str,
    value: &str,
) -> jni::errors::Result<String> {
    android_log(vm, "i", &format!("JNI call {method}"));
    let result = vm.attach_current_thread(|env| {
        let java_value = env.new_string(value)?;
        let method = jni::strings::JNIString::from(method);
        let result = env
            .call_method(
                bridge.as_ref(),
                &method,
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[jni::objects::JValue::Object(&java_value)],
            )?
            .l()?;
        let result = jni::objects::JString::cast_local(env, result)?;
        Ok(result.to_string())
    });
    match result {
        Ok(value) => {
            android_log(
                vm,
                if value.is_empty() { "e" } else { "i" },
                &format!("JNI {method} result={value}"),
            );
            Ok(value)
        }
        Err(error) => {
            android_log(vm, "e", &format!("JNI {method} failed: {error}"));
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{limit_tts_text, QueueAction, SpeechQueue, SpeechSettings};

    #[test]
    fn speech_length_is_bounded_without_splitting_unicode() {
        let text = "🙂".repeat(2_001);
        let clipped = limit_tts_text(&text);
        assert_eq!(clipped.chars().count(), 2_000);
        assert_eq!(clipped.encode_utf16().count(), 4_000);
        assert!(std::str::from_utf8(clipped.as_bytes()).is_ok());
        assert_eq!(limit_tts_text("hola"), "hola");
    }

    #[test]
    fn speech_is_disabled_by_default_and_settings_round_trip() {
        let defaults = SpeechSettings::default();
        assert!(!defaults.enabled);
        assert!(defaults.russian_enabled);
        assert_eq!(defaults.language, "es-ES");
        assert_eq!(defaults.rate, 1.0);
        assert_eq!(defaults.russian_volume, 0.75);
        assert_eq!(SpeechSettings::parse(&defaults.serialize()), defaults);
    }

    #[test]
    fn russian_preferences_round_trip_and_preserve_independent_mute() {
        let settings = SpeechSettings {
            enabled: true,
            russian_enabled: false,
            language: "es-MX".into(),
            rate: 1.25,
            russian_volume: 0.42,
            russian_voice: "ru-ru-x-local".into(),
        };
        assert_eq!(SpeechSettings::parse(&settings.serialize()), settings);
    }

    #[test]
    fn invalid_settings_are_clamped_or_replaced() {
        let settings =
            SpeechSettings::parse("enabled=true\nlanguage=fr-FR\nrate=9\nrussian_volume=0.1\n");
        assert!(settings.enabled);
        assert_eq!(settings.language, "es-ES");
        assert_eq!(settings.rate, 1.5);
        assert_eq!(settings.russian_volume, 0.3);
    }

    #[test]
    fn disabled_transitions_do_not_stop_or_enqueue_speech() {
        let mut queue = SpeechQueue::default();
        assert_eq!(queue.set_enabled(false), None);
        assert_eq!(queue.phrase_changed((1, 2), "hola", "привет"), None);
        assert_eq!(queue.set_enabled(true), None);
        let phrase = queue.phrase_changed((2, 3), "adiós", "пока");
        assert!(
            matches!(phrase, Some(QueueAction::Speak { utterance: 2, spanish, russian, replace: true }) if spanish == "adiós" && russian == "")
        );
        assert_eq!(queue.set_enabled(false), Some(QueueAction::Stop));
        assert_eq!(queue.set_enabled(false), None);
    }

    #[test]
    fn phrase_identity_deduplicates_frame_updates_and_new_phrase_replaces() {
        let mut queue = SpeechQueue::default();
        queue.set_enabled(true);
        queue.russian_enabled = true;
        let first = queue.phrase_changed((10, 20), "hola", "привет");
        assert!(matches!(
            first,
            Some(QueueAction::Speak { utterance: 1, spanish, russian, .. })
                if spanish == "hola" && russian == "привет"
        ));
        assert_eq!(
            queue.phrase_changed((10, 20), "hola at changed position", "привет"),
            None
        );
        let next = queue.phrase_changed((11, 21), "adiós", "пока");
        assert!(
            matches!(next, Some(QueueAction::Speak { utterance: 2, spanish, russian, replace: true }) if spanish == "adiós" && russian == "пока")
        );
        queue.russian_enabled = false;
        let spanish_only = queue.phrase_changed((12, 22), "buenos días", "доброе утро");
        assert!(
            matches!(spanish_only, Some(QueueAction::Speak { spanish, russian, .. }) if spanish == "buenos días" && russian.is_empty())
        );
    }

    #[test]
    fn each_pair_queues_its_own_translation_and_deduplicates_repeat_events() {
        let mut queue = SpeechQueue::default();
        queue.set_enabled(true);
        queue.russian_enabled = true;
        let old = queue.phrase_changed((31, 41), "vieja", "старый");
        let new = queue.phrase_changed((32, 42), "nueva", "новый");
        assert!(matches!(old, Some(QueueAction::Speak { russian, .. }) if russian == "старый"));
        assert!(
            matches!(new, Some(QueueAction::Speak { utterance: 2, russian, replace: true, .. }) if russian == "новый")
        );

        // Repeated current IDs do not reschedule either utterance on frame updates.
        assert_eq!(queue.phrase_changed((32, 42), "nueva", "новый"), None);
        assert_eq!(queue.set_enabled(false), Some(QueueAction::Stop));
    }

    #[test]
    fn fixed_test_phrase_is_independent_of_toggle_and_ticker() {
        let mut queue = SpeechQueue::default();
        assert!(
            matches!(queue.test_voice(), QueueAction::Speak { spanish, russian, replace: true, .. } if spanish == super::TEST_VOICE_PHRASE && russian.is_empty())
        );
    }
}
