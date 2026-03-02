import { getLanguage, setLanguage, getSystemLocale } from "../api";
import enUS from "../../locales/en-US.json";
import zhCN from "../../locales/zh-CN.json";

type TranslationDict = typeof enUS;
type Language = "en-US" | "zh-CN";

const translations: Record<Language, TranslationDict> = {
  "en-US": enUS,
  "zh-CN": zhCN,
};

let currentLanguage: Language = "en-US";
let translationsLoaded = false;

// Get nested value from object using dot notation
function getNestedValue(obj: Record<string, unknown>, path: string): string {
  const keys = path.split(".");
  let result: unknown = obj;
  for (const key of keys) {
    if (result && typeof result === "object" && key in result) {
      result = (result as Record<string, unknown>)[key];
    } else {
      return path; // Return path as fallback
    }
  }
  return typeof result === "string" ? result : path;
}

// Translation function
export function t(key: string): string {
  if (!translationsLoaded) {
    console.warn("Translations not loaded yet");
    return key;
  }
  return getNestedValue(translations[currentLanguage] as unknown as Record<string, unknown>, key);
}

// Get current language
export function getCurrentLanguage(): Language {
  return currentLanguage;
}

// Initialize i18n - loads language from DB or detects system language
export async function initI18n(): Promise<void> {
  try {
    // Try to get saved language from database
    const savedLang = await getLanguage();

    if (savedLang && (savedLang === "en-US" || savedLang === "zh-CN")) {
      currentLanguage = savedLang;
    } else {
      // First launch: detect system language and save it
      const systemLocale = await getSystemLocale();
      currentLanguage = systemLocale.startsWith("zh") ? "zh-CN" : "en-US";
      await setLanguage(currentLanguage);
    }

    translationsLoaded = true;
  } catch (error) {
    console.error("Failed to initialize i18n:", error);
    // Fallback to English
    currentLanguage = "en-US";
    translationsLoaded = true;
  }
}

// Change language and persist to database
export async function changeLanguage(lang: Language): Promise<void> {
  currentLanguage = lang;
  await setLanguage(lang);
}

// Check if translations are loaded
export function isTranslationsLoaded(): boolean {
  return translationsLoaded;
}
