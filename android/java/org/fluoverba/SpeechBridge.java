package org.fluoverba;

import android.content.Context;
import android.os.Bundle;
import android.speech.tts.TextToSpeech;
import android.speech.tts.UtteranceProgressListener;
import android.speech.tts.Voice;
import android.util.Log;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

/** Owns one Android TTS engine for Spanish and Russian speech. */
public final class SpeechBridge {
    private static final String TAG = "FLUOVERBA_TTS";
    private static final String TEST_ES = "Hola. Esta es una prueba de voz de FluoVerba.";
    private static final String TEST_RU = "Здравствуйте. Это проверка русского голоса FluoVerba.";
    private static final AtomicLong TEST_IDS = new AtomicLong();
    private final CountDownLatch initialized = new CountDownLatch(1);
    private final ExecutorService completionExecutor = Executors.newSingleThreadExecutor(r -> {
        Thread thread = new Thread(r, "fluoverba-tts-completion");
        thread.setDaemon(true);
        return thread;
    });
    private final Map<String, Voice> localRussianVoices = new HashMap<>();
    private final SpeechPairSequence sequence = new SpeechPairSequence();
    private TextToSpeech tts;
    private volatile int initStatus = TextToSpeech.ERROR;
    private volatile boolean closed;
    private volatile String selectedSpanishVoice;
    private volatile String selectedRussianVoice;
    private volatile boolean spanishAvailable;
    private volatile float russianVolume = 0.75f;

    public SpeechBridge(Context context) {
        Log.i(TAG, "TTS initialization requested");
        tts = new TextToSpeech(context.getApplicationContext(), status -> {
            initStatus = status;
            Log.i(TAG, "TTS onInit status=" + status + " (SUCCESS=" + TextToSpeech.SUCCESS + ")");
            initialized.countDown();
        });
        String engine = tts.getDefaultEngine();
        Log.i(TAG, "selected engine=" + (engine == null ? "<none>" : engine));
        tts.setOnUtteranceProgressListener(new UtteranceProgressListener() {
            @Override public void onStart(String utteranceId) {
                Log.i(TAG, "onStart utterance=" + utteranceId);
            }

            @Override public void onDone(String utteranceId) {
                Log.i(TAG, "onDone utterance=" + utteranceId);
                SpeechPairSequence.Translation translation = sequence.onDone(utteranceId);
                if (!closed && translation != null) {
                    try {
                        completionExecutor.execute(() -> speakRussianAfterSpanish(translation));
                    } catch (RuntimeException error) {
                        Log.e(TAG, "Could not schedule Russian translation", error);
                        sequence.onError(translation.spanishUtterance);
                    }
                }
            }

            @Override public void onError(String utteranceId) {
                handleUtteranceError(utteranceId, TextToSpeech.ERROR);
            }

            @Override public void onError(String utteranceId, int errorCode) {
                handleUtteranceError(utteranceId, errorCode);
            }
        });
    }

    private void handleUtteranceError(String utteranceId, int errorCode) {
        Log.e(TAG, "onError utterance=" + utteranceId + " code=" + errorCode);
        sequence.onError(utteranceId);
    }

    public boolean awaitInitialization() {
        Log.i(TAG, "waiting for asynchronous TTS onInit");
        try {
            boolean completed = initialized.await(30, TimeUnit.SECONDS);
            boolean ready = completed && initStatus == TextToSpeech.SUCCESS;
            Log.i(TAG, "TTS initialization result ready=" + ready + " status=" + initStatus);
            if (!completed) Log.e(TAG, "TTS initialization timed out; speech is unavailable");
            if (completed && !ready) Log.e(TAG, "TTS engine initialization failed; speech is unavailable");
            return ready;
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            Log.e(TAG, "TTS initialization wait interrupted", error);
            return false;
        }
    }

    /** Select only installed, non-network Spanish voices. */
    public String setLanguage(String requestedTag) {
        Log.i(TAG, "Spanish language request=" + requestedTag);
        if (!ready()) return "";
        try {
            List<Voice> candidates = new ArrayList<>();
            for (Voice voice : tts.getVoices()) {
                if (!"es".equals(voice.getLocale().getLanguage())) continue;
                if (!isInstalledOffline(voice)) continue;
                candidates.add(voice);
            }
            List<String> preference = new ArrayList<>();
            addPreference(preference, requestedTag);
            addPreference(preference, "es-ES");
            addPreference(preference, "es-CO");
            addPreference(preference, "es-MX");
            candidates.sort(Comparator
                    .comparingInt((Voice voice) -> {
                        int rank = preference.indexOf(voice.getLocale().toLanguageTag());
                        return rank < 0 ? preference.size() : rank;
                    })
                    .thenComparing(Voice::getName));
            for (Voice voice : candidates) {
                int result = tts.setVoice(voice);
                Log.i(TAG, "Spanish voice candidate=" + voice.getName() + " language="
                        + voice.getLocale().toLanguageTag() + " networkRequired="
                        + voice.isNetworkConnectionRequired() + " setVoice=" + result);
                if (result == TextToSpeech.SUCCESS) {
                    selectedSpanishVoice = voice.getName();
                    spanishAvailable = true;
                    Log.i(TAG, "selected Spanish voice=" + voice.getName() + " language="
                            + voice.getLocale().toLanguageTag() + " offline=true");
                    return voice.getLocale().toLanguageTag();
                }
            }
        } catch (RuntimeException error) {
            Log.e(TAG, "Spanish offline voice selection failed", error);
        }
        spanishAvailable = false;
        Log.e(TAG, "ERROR: no installed offline Spanish voice; network/cloud voices are disabled");
        return "";
    }

    /** Returns the device's locally installed ru-RU voice IDs, one per line. */
    public String getRussianVoices() {
        localRussianVoices.clear();
        if (!ready()) return "";
        try {
            for (Voice voice : tts.getVoices()) {
                if (!"ru".equals(voice.getLocale().getLanguage())
                        || !"RU".equalsIgnoreCase(voice.getLocale().getCountry())
                        || !isInstalledOffline(voice)) continue;
                localRussianVoices.put(voice.getName(), voice);
                Log.i(TAG, "available local ru-RU voice=" + voice.getName()
                        + " networkRequired=" + voice.isNetworkConnectionRequired());
            }
        } catch (RuntimeException error) {
            Log.e(TAG, "Could not enumerate local ru-RU voices", error);
        }
        List<String> names = new ArrayList<>(localRussianVoices.keySet());
        names.sort(String::compareTo);
        if (names.isEmpty()) {
            Log.e(TAG, "ERROR: no installed offline ru-RU voice is available; install Russian voice data in Android settings. Network/cloud voices are disabled.");
        }
        return String.join("\n", names);
    }

    /** Android Voice exposes no standardized gender metadata; this selects only the user's choice. */
    public String setRussianVoice(String requestedName) {
        if (localRussianVoices.isEmpty()) getRussianVoices();
        if (requestedName != null && localRussianVoices.containsKey(requestedName)) {
            selectedRussianVoice = requestedName;
        } else if (selectedRussianVoice == null || !localRussianVoices.containsKey(selectedRussianVoice)) {
            selectedRussianVoice = localRussianVoices.keySet().stream().sorted().findFirst().orElse(null);
        }
        if (selectedRussianVoice == null) {
            Log.e(TAG, "Russian voice selection failed: no local ru-RU voice");
            return "";
        }
        Log.i(TAG, "selected local Russian voice=" + selectedRussianVoice
                + " (gender is not exposed by Android Voice; no gender claim is made)");
        return selectedRussianVoice;
    }

    private static boolean isInstalledOffline(Voice voice) {
        return !voice.isNetworkConnectionRequired()
                && !voice.getFeatures().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED);
    }

    private boolean ready() {
        if (tts == null || closed || initStatus != TextToSpeech.SUCCESS) {
            Log.e(TAG, "TTS operation rejected: engine is not ready");
            return false;
        }
        return true;
    }

    private static void addPreference(List<String> preferences, String tag) {
        if (tag != null && !tag.isEmpty() && !preferences.contains(tag)) preferences.add(tag);
    }

    public void setRate(float rate) {
        Log.i(TAG, "Spanish speech rate request=" + rate);
        if (ready()) {
            int result = tts.setSpeechRate(Math.max(0.5f, Math.min(1.5f, rate)));
            Log.i(TAG, "speech rate result=" + result);
            if (result == TextToSpeech.ERROR) Log.e(TAG, "could not set speech rate");
        }
    }

    public void setRussianEnabled(boolean enabled) {
        Log.i(TAG, "Russian translation enabled=" + enabled);
        String active = sequence.activeUtterance();
        sequence.setRussianEnabled(enabled);
        if (!enabled && active != null && active.endsWith(":ru") && tts != null) {
            int result = tts.stop();
            Log.i(TAG, "Russian stop return code=" + result + " utterance=" + active);
        }
    }

    public void setRussianVolume(float volume) {
        russianVolume = Math.max(0.3f, Math.min(1.0f, volume));
        Log.i(TAG, "Russian translation volume=" + russianVolume);
    }

    public void stop() {
        Log.i(TAG, "stop requested");
        String active = sequence.activeUtterance();
        if (tts != null && active != null) {
            int result = tts.stop();
            Log.i(TAG, "stop return code=" + result + " utterance=" + active);
            sequence.cancel();
        } else {
            Log.i(TAG, "stop skipped: no active utterance");
            sequence.cancel();
        }
    }

    public int speakPair(String spanish, String russian, String pairId, boolean replaceCurrent) {
        Log.i(TAG, "speak pair id=" + pairId + " replace=" + replaceCurrent
                + " Spanish chars=" + spanish.length() + " Russian chars=" + russian.length());
        if (!ready()) return TextToSpeech.ERROR;
        if (!spanishAvailable) {
            Log.e(TAG, "Spanish utterance rejected: no offline Spanish voice selected");
            return TextToSpeech.ERROR;
        }
        if (replaceCurrent && sequence.activeUtterance() != null) stop();
        if (selectedSpanishVoice != null) {
            Voice voice = findVoice(selectedSpanishVoice);
            if (voice != null) tts.setVoice(voice);
        }
        String utteranceId = sequence.beginPair(pairId, russian);
        Bundle params = new Bundle();
        params.putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, 1.0f);
        int result = tts.speak(spanish, TextToSpeech.QUEUE_ADD, params, utteranceId);
        Log.i(TAG, "Spanish speak return code=" + result + " volume=1.0 queue=ADD");
        if (result == TextToSpeech.ERROR) sequence.onError(utteranceId);
        return result;
    }

    private void speakRussianAfterSpanish(SpeechPairSequence.Translation translation) {
        String pairId = translation.pairId;
        if (closed) {
            sequence.onError(translation.spanishUtterance);
            Log.i(TAG, "stale Russian translation cancelled for pair=" + pairId);
            return;
        }
        Voice voice = selectedRussianVoice == null ? null : localRussianVoices.get(selectedRussianVoice);
        if (voice == null || !isInstalledOffline(voice)) {
            Log.e(TAG, "Russian translation unavailable: choose an installed local ru-RU voice");
            sequence.onError(translation.spanishUtterance);
            return;
        }
        String utteranceId = sequence.beginRussian(translation);
        if (utteranceId == null) {
            Log.i(TAG, "stale Russian translation cancelled for pair=" + pairId);
            return;
        }
        int voiceResult = tts.setVoice(voice);
        Log.i(TAG, "Russian voice for pair=" + pairId + " name=" + voice.getName()
                + " networkRequired=" + voice.isNetworkConnectionRequired() + " setVoice=" + voiceResult);
        if (voiceResult != TextToSpeech.SUCCESS) {
            Log.e(TAG, "Russian translation failed: selected local voice was rejected");
            sequence.onError(utteranceId);
            return;
        }
        Bundle params = new Bundle();
        params.putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, russianVolume);
        int result = tts.speak(translation.text, TextToSpeech.QUEUE_ADD, params, utteranceId);
        Log.i(TAG, "Russian speak return code=" + result + " volume=" + russianVolume + " queue=ADD");
        if (result == TextToSpeech.ERROR) sequence.onError(utteranceId);
    }

    private Voice findVoice(String name) {
        for (Voice voice : tts.getVoices()) if (voice.getName().equals(name)) return voice;
        return null;
    }

    public int testVoice() {
        String id = "test-es-" + TEST_IDS.incrementAndGet();
        Log.i(TAG, "Spanish test voice fixed phrase requested");
        return speakPair(TEST_ES, "", id, true);
    }

    public int testRussianVoice() {
        String id = "test-ru-" + TEST_IDS.incrementAndGet();
        Log.i(TAG, "Russian test voice fixed phrase requested");
        if (!ready()) return TextToSpeech.ERROR;
        if (selectedRussianVoice == null) {
            Log.e(TAG, "Russian test rejected: no local ru-RU voice selected");
            return TextToSpeech.ERROR;
        }
        if (sequence.activeUtterance() != null) stop();
        Voice voice = localRussianVoices.get(selectedRussianVoice);
        if (voice == null || !isInstalledOffline(voice) || tts.setVoice(voice) != TextToSpeech.SUCCESS) {
            Log.e(TAG, "Russian test rejected: selected local voice is unavailable");
            return TextToSpeech.ERROR;
        }
        String utteranceId = sequence.beginRussianTest(id);
        Bundle params = new Bundle();
        params.putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, russianVolume);
        int result = tts.speak(TEST_RU, TextToSpeech.QUEUE_ADD, params, utteranceId);
        Log.i(TAG, "Russian test speak return code=" + result + " volume=" + russianVolume);
        if (result == TextToSpeech.ERROR) sequence.onError(utteranceId);
        return result;
    }

    public void close() {
        Log.i(TAG, "shutdown requested");
        closed = true;
        if (tts != null) {
            if (sequence.activeUtterance() != null) stop();
            tts.shutdown();
            tts = null;
            spanishAvailable = false;
            completionExecutor.shutdownNow();
            Log.i(TAG, "shutdown complete");
        } else {
            completionExecutor.shutdownNow();
            Log.i(TAG, "shutdown skipped: already closed");
        }
    }
}
