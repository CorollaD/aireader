// Open catalogs and libraries that can be added with one click. Every address here was checked
// by hand. Left out: sources that need a login or block apps, and Open Library, whose OPDS feed
// offers loans rather than files.

import type { OpdsCatalog } from "../types";

export type Source = Pick<OpdsCatalog, "title" | "url" | "kind">;

const opds = (title: string, url: string): Source => ({ title, url, kind: "" });
const wikisource = (lang: string, name: string): Source => ({ title: `Wikisource · ${name}`, url: `https://${lang}.wikisource.org/`, kind: "wikisource" });
const gutendex = (lang: string, name: string): Source => ({
  title: `Gutenberg · ${name}`,
  url: `https://gutendex.com/books?languages=${lang}`,
  kind: "gutendex",
});

export const CATALOGS: Source[] = [
  opds("Project Gutenberg", "https://www.gutenberg.org/ebooks/search.opds/"),
  opds("Internet Archive", "https://archive.org/services/opds/"),
  opds("Unglue.it", "https://unglue.it/api/opds/"),
  opds("OpenEdition", "https://opds.openedition.org/"),
  opds("The Anarchist Library", "https://theanarchistlibrary.org/opds"),
];

export const WIKISOURCES: Source[] = [
  wikisource("zh", "中文"),
  wikisource("en", "English"),
  wikisource("de", "Deutsch"),
  wikisource("fr", "Français"),
  wikisource("es", "Español"),
  wikisource("it", "Italiano"),
  wikisource("ja", "日本語"),
];

export const GUTENDEX: Source[] = [
  gutendex("zh", "中文"),
  gutendex("de", "Deutsch"),
  gutendex("fr", "Français"),
  gutendex("es", "Español"),
  gutendex("it", "Italiano"),
];

/** Libraries without a catalog interface: they open in the browser, and files come back by import. */
export const WEBSITES: { title: string; note: string; url: string }[] = [
  { title: "CBETA", note: "中文 · 佛典", url: "https://www.cbeta.org/" },
  { title: "中国哲学书电子化计划", note: "中文 · 先秦至汉代", url: "https://ctext.org/" },
  { title: "识典古籍", note: "中文 · 古籍", url: "https://www.shidianguji.com/" },
  { title: "书格", note: "中文 · 古籍扫描", url: "https://www.shuge.org/" },
  { title: "中华古籍资源库", note: "中文 · 国家图书馆", url: "https://read.nlc.cn/" },
  { title: "漢籍リポジトリ", note: "中文 · 古籍全文", url: "https://github.com/kanripo" },
  { title: "青空文庫", note: "日本語", url: "https://www.aozora.gr.jp/" },
  { title: "Deutsches Textarchiv", note: "Deutsch", url: "https://www.deutschestextarchiv.de/" },
  { title: "Zeno.org", note: "Deutsch", url: "http://www.zeno.org/" },
  { title: "Projekt Gutenberg-DE", note: "Deutsch", url: "https://www.projekt-gutenberg.org/" },
  { title: "Digitale Sammlungen", note: "Deutsch · BSB", url: "https://www.digitale-sammlungen.de/" },
  { title: "Ebooks libres et gratuits", note: "Français", url: "https://beq.ebooksgratuits.com/" },
  { title: "Gallica", note: "Français · BnF", url: "https://gallica.bnf.fr/" },
  { title: "Biblioteca Virtual Miguel de Cervantes", note: "Español", url: "https://www.cervantesvirtual.com/" },
  { title: "Elejandría", note: "Español", url: "https://www.elejandria.com/" },
  { title: "Biblioteca Digital Hispánica", note: "Español · BNE", url: "https://bdh.bne.es/" },
  { title: "Liber Liber", note: "Italiano", url: "https://liberliber.it/" },
  { title: "OpenStax", note: "English · textbooks", url: "https://openstax.org/" },
  { title: "The Online Books Page", note: "English · index", url: "https://digital.library.upenn.edu/books/" },
  { title: "Europeana", note: "Europe", url: "https://www.europeana.eu/" },
  { title: "HathiTrust", note: "Global", url: "https://www.hathitrust.org/" },
];
