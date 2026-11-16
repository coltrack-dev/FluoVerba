package org.fluoverba;

/** Standalone JVM regression tests for the production pair sequence state machine. */
public final class SpeechPairSequenceTest {
    public static void main(String[] args) {
        spanishMustFinishBeforeRussianStarts();
        replacementCancelsStaleTranslation();
        errorsAndStopsDoNotAdvanceToRussian();
        disablingRussianLeavesSpanishActive();
        disablingRussianDuringRussianStopsThatTurn();
    }

    private static void spanishMustFinishBeforeRussianStarts() {
        SpeechPairSequence sequence = new SpeechPairSequence();
        String spanish = sequence.beginPair("10:20:1", "привет");
        check("10:20:1:es".equals(spanish), "Spanish utterance starts first");
        SpeechPairSequence.Translation translation = sequence.onDone(spanish);
        check(translation != null && "привет".equals(translation.text), "Spanish onDone releases translation");
        String russian = sequence.beginRussian(translation);
        check("10:20:1:ru".equals(russian), "Russian starts only after Spanish onDone");
        check(sequence.onDone(spanish) == null, "late Spanish callback cannot restart translation");
    }

    private static void replacementCancelsStaleTranslation() {
        SpeechPairSequence sequence = new SpeechPairSequence();
        String oldSpanish = sequence.beginPair("old", "старый перевод");
        SpeechPairSequence.Translation oldTranslation = sequence.onDone(oldSpanish);
        String newSpanish = sequence.beginPair("new", "новый перевод");
        check(sequence.beginRussian(oldTranslation) == null, "old pair generation is rejected");
        check("new:es".equals(newSpanish), "new Spanish utterance becomes active");
    }

    private static void errorsAndStopsDoNotAdvanceToRussian() {
        SpeechPairSequence sequence = new SpeechPairSequence();
        String spanish = sequence.beginPair("error", "перевод");
        sequence.onError(spanish);
        check(sequence.onDone(spanish) == null, "Spanish error clears pending translation");

        String nextSpanish = sequence.beginPair("stop", "отменённый перевод");
        SpeechPairSequence.Translation pending = sequence.onDone(nextSpanish);
        sequence.cancel();
        check(sequence.beginRussian(pending) == null, "stop cancels pending Russian utterance");
    }

    private static void disablingRussianLeavesSpanishActive() {
        SpeechPairSequence sequence = new SpeechPairSequence();
        String spanish = sequence.beginPair("muted", "не произносить");
        sequence.setRussianEnabled(false);
        check(spanish.equals(sequence.activeUtterance()), "Spanish continues after Russian is disabled");
        check(sequence.onDone(spanish) == null, "disabled Russian is not queued after Spanish");
    }

    private static void disablingRussianDuringRussianStopsThatTurn() {
        SpeechPairSequence sequence = new SpeechPairSequence();
        String spanish = sequence.beginPair("mute-current", "перевод");
        String russian = sequence.beginRussian(sequence.onDone(spanish));
        sequence.setRussianEnabled(false);
        check(sequence.activeUtterance() == null, "disabling Russian cancels the active Russian utterance");
        sequence.onError(russian);
        check(sequence.activeUtterance() == null, "late error callback is harmless");
    }

    private static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
