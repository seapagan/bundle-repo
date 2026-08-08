#![cfg(test)]

pub(crate) struct EncodingFixture {
    pub(crate) name: &'static str,
    pub(crate) bytes: &'static [u8],
    pub(crate) expected: &'static str,
    pub(crate) encoding: &'static str,
}

pub(crate) const JAPANESE: &str = "これは文字コード検出のための日本語の文章です。複数の文を含めて、短い入力による誤判定を避けます。古い文書も正しく読み取ります。\n";
pub(crate) const SIMPLIFIED_CHINESE: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。旧文件也应该被正确读取。\n";
pub(crate) const GB18030_CHINESE: &str = "这是用于字符编码检测的中文文本。它包含多个自然句子，以避免短输入造成误判。扩展字符𠀀用于验证四字节编码。\n";
pub(crate) const TRADITIONAL_CHINESE: &str = "這是用於字元編碼偵測的中文文字。它包含多個自然句子，以避免短輸入造成誤判。舊檔案也應該被正確讀取。\n";
pub(crate) const RUSSIAN: &str = "Это русский текст для проверки определения кодировки. Он содержит несколько естественных предложений. Старые файлы должны читаться правильно.\n";
pub(crate) const WESTERN: &str = "Voici un texte français pour vérifier la détection d’encodage. Il contient plusieurs phrases naturelles. Les fichiers anciens doivent être lus correctement.\n";
pub(crate) const UTF16_TEXT: &str = "UTF-16 text with 日本語, русский текст, and العربية. This fixture contains multiple natural sentences. It verifies byte-order-mark handling.\n";

pub(crate) const SHIFT_JIS_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/shift-jis.txt");
pub(crate) const EUC_JP_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/euc-jp.txt");
pub(crate) const ISO_2022_JP_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/iso-2022-jp.txt");
pub(crate) const GBK_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/gbk.txt");
pub(crate) const GB18030_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/gb18030.txt");
pub(crate) const BIG5_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/big5.txt");
pub(crate) const WINDOWS_1251_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/windows-1251.txt");
pub(crate) const WINDOWS_1252_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/windows-1252.txt");
pub(crate) const UTF16LE_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/utf-16le.txt");
pub(crate) const UTF16BE_BYTES: &[u8] =
    include_bytes!("../fixtures/encodings/utf-16be.txt");

pub(crate) const ENCODING_FIXTURES: [EncodingFixture; 10] = [
    EncodingFixture {
        name: "shift-jis.txt",
        bytes: SHIFT_JIS_BYTES,
        expected: JAPANESE,
        encoding: "Shift_JIS",
    },
    EncodingFixture {
        name: "euc-jp.txt",
        bytes: EUC_JP_BYTES,
        expected: JAPANESE,
        encoding: "EUC-JP",
    },
    EncodingFixture {
        name: "iso-2022-jp.txt",
        bytes: ISO_2022_JP_BYTES,
        expected: JAPANESE,
        encoding: "ISO-2022-JP",
    },
    EncodingFixture {
        name: "gbk.txt",
        bytes: GBK_BYTES,
        expected: SIMPLIFIED_CHINESE,
        encoding: "GBK",
    },
    EncodingFixture {
        name: "gb18030.txt",
        bytes: GB18030_BYTES,
        expected: GB18030_CHINESE,
        encoding: "GBK",
    },
    EncodingFixture {
        name: "big5.txt",
        bytes: BIG5_BYTES,
        expected: TRADITIONAL_CHINESE,
        encoding: "Big5",
    },
    EncodingFixture {
        name: "windows-1251.txt",
        bytes: WINDOWS_1251_BYTES,
        expected: RUSSIAN,
        encoding: "windows-1251",
    },
    EncodingFixture {
        name: "windows-1252.txt",
        bytes: WINDOWS_1252_BYTES,
        expected: WESTERN,
        encoding: "windows-1252",
    },
    EncodingFixture {
        name: "utf-16le.txt",
        bytes: UTF16LE_BYTES,
        expected: UTF16_TEXT,
        encoding: "UTF-16LE",
    },
    EncodingFixture {
        name: "utf-16be.txt",
        bytes: UTF16BE_BYTES,
        expected: UTF16_TEXT,
        encoding: "UTF-16BE",
    },
];
