// Controls shown while the book is read aloud.

import type { UpdateSettings } from "../App";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { Settings } from "../settings";
import type { SpeechState } from "./useSpeech";
import { Select } from "../Select";

const RATES = [0.75, 1, 1.25, 1.5, 2];

export default function ReadAloudBar(props: {
  state: SpeechState;
  voices: SpeechSynthesisVoice[];
  settings: Settings;
  update: UpdateSettings;
  onPlayPause: () => void;
  onStep: (delta: 1 | -1) => void;
  onStop: () => void;
}) {
  const { settings, update } = props;
  const voices = [...props.voices].sort((a, b) => a.lang.localeCompare(b.lang) || a.name.localeCompare(b.name));
  const playing = props.state === "playing";
  return (
    <div className="read-aloud" role="toolbar" aria-label={t("reader.readAloud")}>
      <button className="icon-button" title={t("reader.prevSentence")} aria-label={t("reader.prevSentence")} onClick={() => props.onStep(-1)}>
        <Icon name="skipBack" size={16} />
      </button>
      <button
        className="icon-button"
        title={`${playing ? t("reader.pause") : t("reader.play")} (R)`}
        aria-label={playing ? t("reader.pause") : t("reader.play")}
        onClick={props.onPlayPause}
      >
        <Icon name={playing ? "pause" : "play"} size={16} filled />
      </button>
      <button className="icon-button" title={t("reader.nextSentence")} aria-label={t("reader.nextSentence")} onClick={() => props.onStep(1)}>
        <Icon name="skipForward" size={16} />
      </button>
      <Select
        ghost
        value={String(settings.ttsRate)}
        label={t("reader.speechRate")}
        options={RATES.map((r) => ({ value: String(r), label: `${r}×` }))}
        onChange={(rate) => update({ ttsRate: Number(rate) })}
      />
      {settings.tts.provider === "system" ? (
        <Select
          ghost
          className="read-aloud-voice"
          value={settings.ttsVoice ?? ""}
          label={t("reader.speechVoice")}
          options={[{ value: "", label: t("reader.voiceAuto") }, ...voices.map((v) => ({ value: v.voiceURI, label: `${v.name} (${v.lang})` }))]}
          onChange={(voice) => update({ ttsVoice: voice || null })}
        />
      ) : (
        // An online voice is chosen in Settings → Reading.
        <span className="read-aloud-voice muted small ellipsis" title={settings.tts.voice}>
          {settings.tts.voice}
        </span>
      )}
      <button className="icon-button" title={t("common.close")} aria-label={t("common.close")} onClick={props.onStop}>
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
