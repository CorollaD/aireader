// Progress of the text recognition job, visible from every view.

import { useSyncExternalStore } from "react";
import { t } from "../i18n";
import { ocrProgress, stopOcr, watchOcr } from "./job";

export default function OcrProgressBar() {
  const progress = useSyncExternalStore(watchOcr, ocrProgress);
  if (!progress) return null;
  return (
    <div className="ocr-progress" role="status">
      <span className="ellipsis">
        {progress.loading
          ? t("ocr.loading", { title: progress.title })
          : t("ocr.progress", { title: progress.title, done: progress.done, total: progress.total })}
      </span>
      <div className="ocr-track">
        <div className="ocr-fill" style={{ width: `${(progress.done / progress.total) * 100}%` }} />
      </div>
      <button className="link-button" onClick={stopOcr}>
        {t("common.stop")}
      </button>
    </div>
  );
}
