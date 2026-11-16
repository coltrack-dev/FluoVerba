package org.fluoverba;

/** Small synchronized state machine for one ES utterance followed by its RU translation. */
final class SpeechPairSequence {
    static final class Translation {
        final String pairId;
        final String spanishUtterance;
        final String text;

        Translation(String pairId, String spanishUtterance, String text) {
            this.pairId = pairId;
            this.spanishUtterance = spanishUtterance;
            this.text = text;
        }
    }

    private boolean russianEnabled = true;
    private String pairId;
    private String spanishUtterance;
    private String pendingRussian;
    private String activeUtterance;

    synchronized String beginPair(String nextPairId, String russianText) {
        pairId = nextPairId;
        spanishUtterance = nextPairId + ":es";
        activeUtterance = spanishUtterance;
        pendingRussian = russianEnabled && russianText != null && !russianText.isEmpty()
                ? russianText : null;
        return spanishUtterance;
    }

    synchronized String beginRussianTest(String testId) {
        pairId = testId;
        spanishUtterance = null;
        pendingRussian = null;
        activeUtterance = testId + ":ru";
        return activeUtterance;
    }

    synchronized Translation onDone(String utteranceId) {
        if (activeUtterance == null || !activeUtterance.equals(utteranceId)) return null;
        if (spanishUtterance != null && spanishUtterance.equals(utteranceId)) {
            if (russianEnabled && pendingRussian != null) {
                return new Translation(pairId, spanishUtterance, pendingRussian);
            }
        }
        clear();
        return null;
    }

    synchronized String beginRussian(Translation translation) {
        if (translation == null || pairId == null || !pairId.equals(translation.pairId)
                || !russianEnabled || pendingRussian == null
                || !pendingRussian.equals(translation.text)
                || activeUtterance == null
                || !activeUtterance.equals(translation.spanishUtterance)) return null;
        activeUtterance = pairId + ":ru";
        return activeUtterance;
    }

    synchronized void setRussianEnabled(boolean enabled) {
        russianEnabled = enabled;
        if (!enabled) {
            pendingRussian = null;
            if (activeUtterance != null && activeUtterance.endsWith(":ru")) clear();
        }
    }

    synchronized void onError(String utteranceId) {
        if (utteranceId != null && utteranceId.equals(activeUtterance)) clear();
    }

    synchronized void cancel() {
        clear();
    }

    synchronized String activeUtterance() {
        return activeUtterance;
    }

    private void clear() {
        pairId = null;
        spanishUtterance = null;
        pendingRussian = null;
        activeUtterance = null;
    }
}
