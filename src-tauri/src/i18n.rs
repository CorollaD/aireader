//! Messages the backend shows to the user (errors, generated titles, exported notes), in the
//! interface language the frontend sets with `set_locale`.

use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Interface languages, in the column order of [`MESSAGES`].
pub const LANGS: [(&str, &str); 10] = [
    ("zh-CN", "Simplified Chinese"),
    ("zh-TW", "Traditional Chinese"),
    ("en", "English"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("fr", "French"),
    ("de", "German"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("pt", "Portuguese"),
];

const ENGLISH: usize = 2;
static CURRENT: AtomicUsize = AtomicUsize::new(ENGLISH);

/// Sets the language by code ("zh-CN", "en", …); unknown codes fall back to English.
pub fn set_locale(code: &str) {
    let i = LANGS
        .iter()
        .position(|(c, _)| *c == code)
        .unwrap_or(ENGLISH);
    CURRENT.store(i, Ordering::Relaxed);
}

/// The current interface language in English ("Simplified Chinese"), for AI prompts.
pub fn language_name() -> &'static str {
    LANGS[CURRENT.load(Ordering::Relaxed)].1
}

/// The message for `key` in the current language.
pub fn tr(key: &str) -> String {
    lookup(key, CURRENT.load(Ordering::Relaxed)).to_owned()
}

/// The message for `key` with `{name}` placeholders filled in.
pub fn tr_with(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut s = tr(key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

fn lookup(key: &str, lang: usize) -> &'static str {
    match MESSAGES.iter().find(|(k, _)| *k == key) {
        Some((_, texts)) => texts[lang],
        None => {
            debug_assert!(false, "missing message {key}");
            "?"
        }
    }
}

/// `t!("key")` or `t!("key", name = value, …)`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_with($key, &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}

/// Columns: zh-CN, zh-TW, en, es, it, fr, de, ja, ko, pt.
const MESSAGES: &[(&str, [&str; 10])] = &[
    (
        "book_not_found",
        [
            "书籍不存在",
            "書籍不存在",
            "Book not found",
            "No se encuentra el libro",
            "Libro non trovato",
            "Livre introuvable",
            "Buch nicht gefunden",
            "本が見つかりません",
            "책을 찾을 수 없습니다",
            "Livro não encontrado",
        ],
    ),
    (
        "book_file_missing",
        [
            "书籍文件已丢失：{path}",
            "書籍檔案已遺失：{path}",
            "The book's file is missing: {path}",
            "Falta el archivo del libro: {path}",
            "Il file del libro non c'è più: {path}",
            "Le fichier du livre est introuvable : {path}",
            "Die Datei des Buchs fehlt: {path}",
            "本のファイルが見つかりません：{path}",
            "책 파일이 없습니다: {path}",
            "O arquivo do livro não foi encontrado: {path}",
        ],
    ),
    (
        "open_failed",
        [
            "无法打开 {path}",
            "無法開啟 {path}",
            "Couldn't open {path}",
            "No se pudo abrir {path}",
            "Impossibile aprire {path}",
            "Impossible d'ouvrir {path}",
            "{path} konnte nicht geöffnet werden",
            "{path} を開けませんでした",
            "{path}을(를) 열 수 없습니다",
            "Não foi possível abrir {path}",
        ],
    ),
    (
        "read_failed",
        [
            "无法读取文件",
            "無法讀取檔案",
            "Couldn't read the file",
            "No se pudo leer el archivo",
            "Impossibile leggere il file",
            "Impossible de lire le fichier",
            "Die Datei konnte nicht gelesen werden",
            "ファイルを読み込めませんでした",
            "파일을 읽을 수 없습니다",
            "Não foi possível ler o arquivo",
        ],
    ),
    (
        "pdf_encrypted",
        [
            "PDF 已加密，暂不支持输入密码",
            "PDF 已加密，暫不支援輸入密碼",
            "This PDF is encrypted; password-protected PDFs aren't supported yet",
            "Este PDF está cifrado; aún no se admiten PDF con contraseña",
            "Questo PDF è cifrato; i PDF protetti da password non sono ancora supportati",
            "Ce PDF est chiffré ; les PDF protégés par mot de passe ne sont pas encore pris en charge",
            "Dieses PDF ist verschlüsselt; passwortgeschützte PDFs werden noch nicht unterstützt",
            "この PDF は暗号化されています。パスワード付き PDF にはまだ対応していません",
            "암호화된 PDF입니다. 암호가 걸린 PDF는 아직 지원하지 않습니다",
            "Este PDF está criptografado; PDFs com senha ainda não são suportados",
        ],
    ),
    (
        "pdf_open_failed",
        [
            "PDF 打开失败：{error}",
            "PDF 開啟失敗：{error}",
            "Couldn't open the PDF: {error}",
            "No se pudo abrir el PDF: {error}",
            "Impossibile aprire il PDF: {error}",
            "Impossible d'ouvrir le PDF : {error}",
            "Das PDF konnte nicht geöffnet werden: {error}",
            "PDF を開けませんでした：{error}",
            "PDF를 열 수 없습니다: {error}",
            "Não foi possível abrir o PDF: {error}",
        ],
    ),
    (
        "parse_failed",
        [
            "{format} 解析失败：{error}",
            "{format} 解析失敗：{error}",
            "Couldn't read the {format}: {error}",
            "No se pudo leer el {format}: {error}",
            "Impossibile leggere il file {format}: {error}",
            "Impossible de lire le {format} : {error}",
            "{format} konnte nicht gelesen werden: {error}",
            "{format} を解析できませんでした：{error}",
            "{format}을(를) 해석할 수 없습니다: {error}",
            "Não foi possível ler o {format}: {error}",
        ],
    ),
    (
        "no_content",
        [
            "{format} 中没有可显示的内容",
            "{format} 中沒有可顯示的內容",
            "The {format} has nothing to display",
            "El {format} no tiene contenido que mostrar",
            "Il file {format} non ha contenuti da mostrare",
            "Le {format} ne contient rien à afficher",
            "{format} enthält nichts Anzeigbares",
            "{format} に表示できる内容がありません",
            "{format}에 표시할 내용이 없습니다",
            "O {format} não tem conteúdo para exibir",
        ],
    ),
    (
        "archive_no_fb2",
        [
            "压缩包中没有 .fb2 文件",
            "壓縮檔中沒有 .fb2 檔案",
            "The archive contains no .fb2 file",
            "El archivo comprimido no contiene ningún .fb2",
            "L'archivio non contiene file .fb2",
            "L'archive ne contient aucun fichier .fb2",
            "Das Archiv enthält keine .fb2-Datei",
            "アーカイブに .fb2 ファイルがありません",
            "압축 파일에 .fb2 파일이 없습니다",
            "O arquivo compactado não contém .fb2",
        ],
    ),
    (
        "archive_no_images",
        [
            "压缩包中没有图片",
            "壓縮檔中沒有圖片",
            "The archive contains no images",
            "El archivo comprimido no contiene imágenes",
            "L'archivio non contiene immagini",
            "L'archive ne contient aucune image",
            "Das Archiv enthält keine Bilder",
            "アーカイブに画像がありません",
            "압축 파일에 이미지가 없습니다",
            "O arquivo compactado não contém imagens",
        ],
    ),
    (
        "mobi_drm",
        [
            "此书受 DRM 保护，无法打开",
            "此書受 DRM 保護，無法開啟",
            "This book is DRM-protected and can't be opened",
            "Este libro tiene DRM y no se puede abrir",
            "Questo libro è protetto da DRM e non si può aprire",
            "Ce livre est protégé par DRM et ne peut pas être ouvert",
            "Dieses Buch ist DRM-geschützt und kann nicht geöffnet werden",
            "この本は DRM で保護されているため開けません",
            "DRM으로 보호된 책이라 열 수 없습니다",
            "Este livro tem DRM e não pode ser aberto",
        ],
    ),
    (
        "mobi_decode",
        [
            "无法解码 MOBI 文本内容",
            "無法解碼 MOBI 文字內容",
            "Couldn't decode the MOBI text",
            "No se pudo descodificar el texto MOBI",
            "Impossibile decodificare il testo MOBI",
            "Impossible de décoder le texte MOBI",
            "Der MOBI-Text konnte nicht dekodiert werden",
            "MOBI のテキストをデコードできませんでした",
            "MOBI 텍스트를 디코딩할 수 없습니다",
            "Não foi possível decodificar o texto MOBI",
        ],
    ),
    (
        "document_empty",
        [
            "文档为空",
            "文件為空",
            "The document is empty",
            "El documento está vacío",
            "Il documento è vuoto",
            "Le document est vide",
            "Das Dokument ist leer",
            "ドキュメントが空です",
            "문서가 비어 있습니다",
            "O documento está vazio",
        ],
    ),
    (
        "library_corrupt",
        [
            "library.json 已损坏",
            "library.json 已損毀",
            "library.json is damaged",
            "library.json está dañado",
            "library.json è danneggiato",
            "library.json est endommagé",
            "library.json ist beschädigt",
            "library.json が壊れています",
            "library.json이 손상되었습니다",
            "library.json está corrompido",
        ],
    ),
    (
        "invalid_file_name",
        [
            "无效的文件名",
            "無效的檔名",
            "Invalid file name",
            "Nombre de archivo no válido",
            "Nome file non valido",
            "Nom de fichier non valide",
            "Ungültiger Dateiname",
            "無効なファイル名です",
            "잘못된 파일 이름입니다",
            "Nome de arquivo inválido",
        ],
    ),
    (
        "copy_failed",
        [
            "复制文件失败",
            "拷貝檔案失敗",
            "Couldn't copy the file",
            "No se pudo copiar el archivo",
            "Impossibile copiare il file",
            "Impossible de copier le fichier",
            "Die Datei konnte nicht kopiert werden",
            "ファイルをコピーできませんでした",
            "파일을 복사할 수 없습니다",
            "Não foi possível copiar o arquivo",
        ],
    ),
    (
        "title_empty",
        [
            "书名不能为空",
            "書名不能為空",
            "The title can't be empty",
            "El título no puede estar vacío",
            "Il titolo non può essere vuoto",
            "Le titre ne peut pas être vide",
            "Der Titel darf nicht leer sein",
            "書名を入力してください",
            "제목을 입력하세요",
            "O título não pode ficar vazio",
        ],
    ),
    (
        "group_empty",
        [
            "分组名不能为空",
            "分組名稱不能為空",
            "The group name can't be empty",
            "El nombre del grupo no puede estar vacío",
            "Il nome del gruppo non può essere vuoto",
            "Le nom du groupe ne peut pas être vide",
            "Der Gruppenname darf nicht leer sein",
            "グループ名を入力してください",
            "그룹 이름을 입력하세요",
            "O nome do grupo não pode ficar vazio",
        ],
    ),
    (
        "group_exists",
        [
            "分组「{name}」已存在",
            "分組「{name}」已存在",
            "The group “{name}” already exists",
            "El grupo «{name}» ya existe",
            "Il gruppo «{name}» esiste già",
            "Le groupe « {name} » existe déjà",
            "Die Gruppe „{name}“ gibt es schon",
            "グループ「{name}」はすでにあります",
            "'{name}' 그룹이 이미 있습니다",
            "O grupo “{name}” já existe",
        ],
    ),
    (
        "group_not_found",
        [
            "分组不存在",
            "分組不存在",
            "Group not found",
            "No se encuentra el grupo",
            "Gruppo non trovato",
            "Groupe introuvable",
            "Gruppe nicht gefunden",
            "グループが見つかりません",
            "그룹을 찾을 수 없습니다",
            "Grupo não encontrado",
        ],
    ),
    (
        "chapter_not_found",
        [
            "章节不存在",
            "章節不存在",
            "Chapter not found",
            "No se encuentra el capítulo",
            "Capitolo non trovato",
            "Chapitre introuvable",
            "Kapitel nicht gefunden",
            "章が見つかりません",
            "장을 찾을 수 없습니다",
            "Capítulo não encontrado",
        ],
    ),
    (
        "not_text_book",
        [
            "这本书不是文字类书籍",
            "這本書不是文字類書籍",
            "This book has no reflowable text",
            "Este libro no tiene texto adaptable",
            "Questo libro non ha testo adattabile",
            "Ce livre n'a pas de texte recomposable",
            "Dieses Buch hat keinen umbrechbaren Text",
            "この本はリフロー型のテキストではありません",
            "이 책은 텍스트형 책이 아닙니다",
            "Este livro não tem texto ajustável",
        ],
    ),
    (
        "not_paged_book",
        [
            "这本书不是按页显示的文档",
            "這本書不是按頁顯示的文件",
            "This book isn't a page-based document",
            "Este libro no es un documento paginado",
            "Questo libro non è un documento a pagine",
            "Ce livre n'est pas un document paginé",
            "Dieses Buch ist kein seitenbasiertes Dokument",
            "この本はページ単位の文書ではありません",
            "이 책은 쪽 단위 문서가 아닙니다",
            "Este livro não é um documento paginado",
        ],
    ),
    (
        "title_start",
        [
            "开始",
            "開始",
            "Start",
            "Inicio",
            "Inizio",
            "Début",
            "Anfang",
            "はじめ",
            "시작",
            "Início",
        ],
    ),
    (
        "title_part",
        [
            "第 {n} 部分",
            "第 {n} 部分",
            "Part {n}",
            "Parte {n}",
            "Parte {n}",
            "Partie {n}",
            "Teil {n}",
            "第 {n} 部",
            "{n}부",
            "Parte {n}",
        ],
    ),
    (
        "title_section",
        [
            "第 {n} 节",
            "第 {n} 節",
            "Section {n}",
            "Sección {n}",
            "Sezione {n}",
            "Section {n}",
            "Abschnitt {n}",
            "第 {n} 節",
            "{n}절",
            "Seção {n}",
        ],
    ),
    (
        "title_chapter",
        [
            "第 {n} 章",
            "第 {n} 章",
            "Chapter {n}",
            "Capítulo {n}",
            "Capitolo {n}",
            "Chapitre {n}",
            "Kapitel {n}",
            "第 {n} 章",
            "{n}장",
            "Capítulo {n}",
        ],
    ),
    (
        "title_page",
        [
            "第 {n} 页",
            "第 {n} 頁",
            "Page {n}",
            "Página {n}",
            "Pagina {n}",
            "Page {n}",
            "Seite {n}",
            "{n} ページ",
            "{n}쪽",
            "Página {n}",
        ],
    ),
    (
        "title_pages",
        [
            "第 {from}–{to} 页",
            "第 {from}–{to} 頁",
            "Pages {from}–{to}",
            "Páginas {from}–{to}",
            "Pagine {from}–{to}",
            "Pages {from}–{to}",
            "Seiten {from}–{to}",
            "{from}–{to} ページ",
            "{from}–{to}쪽",
            "Páginas {from}–{to}",
        ],
    ),
    (
        "notes_bookmarks",
        [
            "书签",
            "書籤",
            "Bookmarks",
            "Marcadores",
            "Segnalibri",
            "Signets",
            "Lesezeichen",
            "ブックマーク",
            "책갈피",
            "Marcadores",
        ],
    ),
    (
        "notes_file",
        [
            "{title} 笔记.md",
            "{title} 筆記.md",
            "{title} notes.md",
            "{title} notas.md",
            "{title} note.md",
            "{title} notes.md",
            "{title} Notizen.md",
            "{title} メモ.md",
            "{title} 메모.md",
            "{title} notas.md",
        ],
    ),
    (
        "ai_consent_required",
        [
            "使用云端 AI 服务前，需要先同意发送书籍内容",
            "使用雲端 AI 服務前，需要先同意傳送書籍內容",
            "Before using a cloud AI service, agree to sending book content",
            "Antes de usar un servicio de IA en la nube, acepta el envío del contenido del libro",
            "Prima di usare un servizio di IA cloud, accetta l'invio del contenuto del libro",
            "Avant d'utiliser un service d'IA dans le cloud, acceptez l'envoi du contenu du livre",
            "Stimme vor der Nutzung eines Cloud-KI-Dienstes dem Senden von Buchinhalten zu",
            "クラウドの AI サービスを使う前に、本の内容の送信に同意してください",
            "클라우드 AI 서비스를 사용하려면 먼저 책 내용 전송에 동의해야 합니다",
            "Antes de usar um serviço de IA na nuvem, concorde com o envio do conteúdo do livro",
        ],
    ),
    (
        "ai_keychain_failed",
        [
            "无法保存到系统钥匙串：{error}",
            "無法儲存到系統鑰匙圈：{error}",
            "Couldn't save to the system keychain: {error}",
            "No se pudo guardar en el llavero del sistema: {error}",
            "Impossibile salvare nel portachiavi di sistema: {error}",
            "Impossible d'enregistrer dans le trousseau du système : {error}",
            "Speichern im Schlüsselbund des Systems fehlgeschlagen: {error}",
            "システムのキーチェーンに保存できませんでした：{error}",
            "시스템 키체인에 저장할 수 없습니다: {error}",
            "Não foi possível salvar nas chaves do sistema: {error}",
        ],
    ),
    (
        "ai_nothing_to_organize",
        [
            "没有要整理的书",
            "沒有要整理的書",
            "No books to organize",
            "No hay libros que organizar",
            "Nessun libro da organizzare",
            "Aucun livre à ranger",
            "Keine Bücher zum Ordnen",
            "整理する本がありません",
            "정리할 책이 없습니다",
            "Nenhum livro para organizar",
        ],
    ),
    (
        "ai_not_configured",
        [
            "还没有配置 AI 服务，请先在「AI 设置」中选择服务和模型",
            "還沒有設定 AI 服務，請先在「AI 設定」中選擇服務和模型",
            "No AI service is set up yet. Choose a service and model in AI settings.",
            "Aún no hay ningún servicio de IA configurado. Elige un servicio y un modelo en los ajustes de IA.",
            "Nessun servizio di IA configurato. Scegli un servizio e un modello nelle impostazioni IA.",
            "Aucun service d'IA n'est configuré. Choisissez un service et un modèle dans les réglages IA.",
            "Es ist noch kein KI-Dienst eingerichtet. Wähle in den KI-Einstellungen einen Dienst und ein Modell.",
            "AI サービスが未設定です。AI 設定でサービスとモデルを選んでください。",
            "AI 서비스가 설정되지 않았습니다. AI 설정에서 서비스와 모델을 선택하세요.",
            "Nenhum serviço de IA configurado. Escolha um serviço e um modelo nas configurações de IA.",
        ],
    ),
    (
        "ai_custom_needs_url",
        [
            "自定义服务需要填写接口地址",
            "自訂服務需要填寫端點網址",
            "A custom service needs an endpoint URL",
            "Un servicio personalizado necesita una URL",
            "Un servizio personalizzato richiede un URL",
            "Un service personnalisé nécessite une URL",
            "Ein eigener Dienst braucht eine Endpunkt-URL",
            "カスタムサービスにはエンドポイント URL が必要です",
            "사용자 지정 서비스에는 엔드포인트 URL이 필요합니다",
            "Um serviço personalizado precisa de uma URL",
        ],
    ),
    (
        "ai_missing_key",
        [
            "缺少 API Key：请在「AI 设置」中填写",
            "缺少 API Key：請在「AI 設定」中填寫",
            "Missing API key: enter it in AI settings",
            "Falta la clave de API: introdúcela en los ajustes de IA",
            "Manca la chiave API: inseriscila nelle impostazioni IA",
            "Clé d'API manquante : saisissez-la dans les réglages IA",
            "API-Schlüssel fehlt: trage ihn in den KI-Einstellungen ein",
            "API キーがありません。AI 設定で入力してください",
            "API 키가 없습니다. AI 설정에서 입력하세요",
            "Chave de API ausente: informe-a nas configurações de IA",
        ],
    ),
    (
        "ai_connect_failed",
        [
            "无法连接模型 {model}：{error}",
            "無法連線模型 {model}：{error}",
            "Couldn't connect to model {model}: {error}",
            "No se pudo conectar con el modelo {model}: {error}",
            "Impossibile collegarsi al modello {model}: {error}",
            "Impossible de se connecter au modèle {model} : {error}",
            "Verbindung zum Modell {model} fehlgeschlagen: {error}",
            "モデル {model} に接続できません：{error}",
            "{model} 모델에 연결할 수 없습니다: {error}",
            "Não foi possível conectar ao modelo {model}: {error}",
        ],
    ),
    (
        "ai_bad_key",
        [
            "API Key 无效或没有权限，请在「AI 设置」中检查",
            "API Key 無效或沒有權限，請在「AI 設定」中檢查",
            "The API key is invalid or lacks permission. Check it in AI settings.",
            "La clave de API no es válida o no tiene permiso. Revísala en los ajustes de IA.",
            "La chiave API non è valida o non ha i permessi. Controllala nelle impostazioni IA.",
            "La clé d'API est invalide ou n'a pas les droits. Vérifiez-la dans les réglages IA.",
            "Der API-Schlüssel ist ungültig oder hat keine Berechtigung. Prüfe ihn in den KI-Einstellungen.",
            "API キーが無効か、権限がありません。AI 設定で確認してください。",
            "API 키가 잘못되었거나 권한이 없습니다. AI 설정에서 확인하세요.",
            "A chave de API é inválida ou não tem permissão. Verifique nas configurações de IA.",
        ],
    ),
    (
        "ai_local_unreachable",
        [
            "连接不到本地模型服务，请确认 Ollama / llama.cpp 已经启动，并检查接口地址",
            "連線不到本機模型服務，請確認 Ollama / llama.cpp 已經啟動，並檢查端點網址",
            "Can't reach the local model service. Make sure Ollama / llama.cpp is running and check the endpoint URL.",
            "No se puede conectar con el servicio de modelos local. Comprueba que Ollama / llama.cpp está en marcha y revisa la URL.",
            "Impossibile raggiungere il servizio di modelli locale. Verifica che Ollama / llama.cpp sia avviato e controlla l'URL.",
            "Impossible de joindre le service de modèles local. Vérifiez qu'Ollama / llama.cpp est lancé et contrôlez l'URL.",
            "Der lokale Modelldienst ist nicht erreichbar. Prüfe, ob Ollama / llama.cpp läuft, und kontrolliere die URL.",
            "ローカルのモデルサービスに接続できません。Ollama / llama.cpp が起動しているか、URL を確認してください。",
            "로컬 모델 서비스에 연결할 수 없습니다. Ollama / llama.cpp가 실행 중인지, 엔드포인트 URL이 맞는지 확인하세요.",
            "Não foi possível acessar o serviço de modelos local. Verifique se o Ollama / llama.cpp está em execução e confira a URL.",
        ],
    ),
    (
        "ai_network",
        [
            "网络错误，连接不到 AI 服务，请检查网络或接口地址",
            "網路錯誤，連線不到 AI 服務，請檢查網路或端點網址",
            "Network error: can't reach the AI service. Check your connection or the endpoint URL.",
            "Error de red: no se puede conectar con el servicio de IA. Revisa la conexión o la URL.",
            "Errore di rete: impossibile raggiungere il servizio di IA. Controlla la connessione o l'URL.",
            "Erreur réseau : impossible de joindre le service d'IA. Vérifiez la connexion ou l'URL.",
            "Netzwerkfehler: Der KI-Dienst ist nicht erreichbar. Prüfe die Verbindung oder die URL.",
            "ネットワークエラー：AI サービスに接続できません。接続状況か URL を確認してください。",
            "네트워크 오류: AI 서비스에 연결할 수 없습니다. 네트워크나 엔드포인트 URL을 확인하세요.",
            "Erro de rede: não foi possível acessar o serviço de IA. Verifique a conexão ou a URL.",
        ],
    ),
    (
        "ai_rate_limited",
        [
            "请求过于频繁或额度不足，请稍后再试或检查账户余额",
            "請求過於頻繁或額度不足，請稍後再試或檢查帳戶餘額",
            "Too many requests or quota used up. Try again later or check your account balance.",
            "Demasiadas solicitudes o cuota agotada. Inténtalo más tarde o revisa el saldo de tu cuenta.",
            "Troppe richieste o quota esaurita. Riprova più tardi o controlla il saldo dell'account.",
            "Trop de requêtes ou quota épuisé. Réessayez plus tard ou vérifiez le solde de votre compte.",
            "Zu viele Anfragen oder Kontingent aufgebraucht. Versuche es später erneut oder prüfe dein Guthaben.",
            "リクエストが多すぎるか、利用枠が不足しています。しばらくしてから再試行するか、残高を確認してください。",
            "요청이 너무 많거나 한도가 부족합니다. 잠시 후 다시 시도하거나 계정 잔액을 확인하세요.",
            "Solicitações demais ou cota esgotada. Tente mais tarde ou verifique o saldo da conta.",
        ],
    ),
    (
        "ai_model_not_found",
        [
            "找不到模型「{model}」，请检查模型名称",
            "找不到模型「{model}」，請檢查模型名稱",
            "Model “{model}” not found. Check the model name.",
            "No se encuentra el modelo «{model}». Revisa el nombre.",
            "Modello «{model}» non trovato. Controlla il nome.",
            "Modèle « {model} » introuvable. Vérifiez son nom.",
            "Modell „{model}“ nicht gefunden. Prüfe den Namen.",
            "モデル「{model}」が見つかりません。モデル名を確認してください。",
            "'{model}' 모델을 찾을 수 없습니다. 모델 이름을 확인하세요.",
            "Modelo “{model}” não encontrado. Verifique o nome.",
        ],
    ),
    (
        "ai_too_long",
        [
            "内容超出模型能处理的长度",
            "內容超出模型能處理的長度",
            "The text is longer than the model can handle",
            "El texto es más largo de lo que el modelo puede procesar",
            "Il testo è più lungo di quanto il modello possa gestire",
            "Le texte dépasse ce que le modèle peut traiter",
            "Der Text ist länger, als das Modell verarbeiten kann",
            "テキストがモデルの処理できる長さを超えています",
            "내용이 모델이 처리할 수 있는 길이를 넘었습니다",
            "O texto é maior do que o modelo consegue processar",
        ],
    ),
    (
        "ai_service_error",
        [
            "AI 服务返回错误：{detail}",
            "AI 服務傳回錯誤：{detail}",
            "The AI service returned an error: {detail}",
            "El servicio de IA devolvió un error: {detail}",
            "Il servizio di IA ha restituito un errore: {detail}",
            "Le service d'IA a renvoyé une erreur : {detail}",
            "Der KI-Dienst meldet einen Fehler: {detail}",
            "AI サービスがエラーを返しました：{detail}",
            "AI 서비스에서 오류가 발생했습니다: {detail}",
            "O serviço de IA retornou um erro: {detail}",
        ],
    ),
    (
        "ai_chapter_empty",
        [
            "这一章没有文字内容",
            "這一章沒有文字內容",
            "This chapter has no text",
            "Este capítulo no tiene texto",
            "Questo capitolo non ha testo",
            "Ce chapitre ne contient pas de texte",
            "Dieses Kapitel enthält keinen Text",
            "この章には文字がありません",
            "이 장에는 텍스트가 없습니다",
            "Este capítulo não tem texto",
        ],
    ),
    (
        "ai_pages_no_text",
        [
            "这些页面没有可提取的文字（可能是扫描版或图片）",
            "這些頁面沒有可擷取的文字（可能是掃描版或圖片）",
            "These pages have no extractable text (they may be scans or images)",
            "Estas páginas no tienen texto extraíble (pueden ser escaneos o imágenes)",
            "Queste pagine non hanno testo estraibile (potrebbero essere scansioni o immagini)",
            "Ces pages n'ont pas de texte extractible (il s'agit peut-être de scans ou d'images)",
            "Diese Seiten enthalten keinen auslesbaren Text (vielleicht Scans oder Bilder)",
            "これらのページには抽出できる文字がありません（スキャンや画像の可能性があります）",
            "이 쪽들에는 추출할 수 있는 텍스트가 없습니다(스캔본이나 이미지일 수 있습니다)",
            "Estas páginas não têm texto extraível (podem ser digitalizações ou imagens)",
        ],
    ),
    (
        "ai_book_no_text",
        [
            "这本书没有可以总结的文字（可能是扫描版或图片）",
            "這本書沒有可以摘要的文字（可能是掃描版或圖片）",
            "This book has no text to summarize (it may be scans or images)",
            "Este libro no tiene texto que resumir (puede ser un escaneo o imágenes)",
            "Questo libro non ha testo da riassumere (potrebbe essere una scansione o immagini)",
            "Ce livre n'a pas de texte à résumer (il s'agit peut-être de scans ou d'images)",
            "Dieses Buch enthält keinen Text zum Zusammenfassen (vielleicht Scans oder Bilder)",
            "この本には要約できる文字がありません（スキャンや画像の可能性があります）",
            "이 책에는 요약할 텍스트가 없습니다(스캔본이나 이미지일 수 있습니다)",
            "Este livro não tem texto para resumir (pode ser digitalização ou imagens)",
        ],
    ),
    (
        "ai_summarizing_part",
        [
            "「{title}」第 {i}/{n} 部分",
            "「{title}」第 {i}/{n} 部分",
            "“{title}” part {i}/{n}",
            "«{title}» parte {i}/{n}",
            "«{title}» parte {i}/{n}",
            "« {title} » partie {i}/{n}",
            "„{title}“ Teil {i}/{n}",
            "「{title}」{i}/{n} 部",
            "'{title}' {i}/{n}부",
            "“{title}” parte {i}/{n}",
        ],
    ),
    (
        "ai_summarizing",
        [
            "正在总结「{title}」",
            "正在摘要「{title}」",
            "Summarizing “{title}”",
            "Resumiendo «{title}»",
            "Riassunto di «{title}»",
            "Résumé de « {title} »",
            "„{title}“ wird zusammengefasst",
            "「{title}」を要約中",
            "'{title}' 요약 중",
            "Resumindo “{title}”",
        ],
    ),
    (
        "ai_combining",
        [
            "正在汇总全书",
            "正在彙整全書",
            "Combining into a book summary",
            "Combinando en un resumen del libro",
            "Unione nel riassunto del libro",
            "Assemblage du résumé du livre",
            "Wird zur Buchzusammenfassung zusammengeführt",
            "本全体の要約にまとめています",
            "책 전체 요약으로 합치는 중",
            "Combinando no resumo do livro",
        ],
    ),
    (
        "ai_no_question",
        [
            "没有要发送的问题",
            "沒有要傳送的問題",
            "There's no question to send",
            "No hay ninguna pregunta que enviar",
            "Non c'è nessuna domanda da inviare",
            "Aucune question à envoyer",
            "Es gibt keine Frage zum Senden",
            "送信する質問がありません",
            "보낼 질문이 없습니다",
            "Não há pergunta para enviar",
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        s.split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned()))
            .collect()
    }

    #[test]
    fn every_language_has_the_same_placeholders() {
        let mut keys = BTreeSet::new();
        for (key, texts) in MESSAGES {
            assert!(keys.insert(key), "duplicate key {key}");
            let expected = placeholders(texts[ENGLISH]);
            for (i, text) in texts.iter().enumerate() {
                assert!(!text.is_empty(), "{key} is empty in {}", LANGS[i].0);
                assert_eq!(placeholders(text), expected, "{key} in {}", LANGS[i].0);
            }
        }
    }

    #[test]
    fn fills_placeholders() {
        // The default language is English; tests never change it.
        assert_eq!(t!("title_pages", from = 3, to = 5), "Pages 3–5");
        assert_eq!(tr("title_start"), "Start");
        assert_eq!(lookup("title_start", 0), "开始");
    }
}
