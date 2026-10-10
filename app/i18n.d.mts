export type Language = "en" | "zh";
export function getLanguage(): Language;
export function getLocale(): "en-US" | "zh-CN";
export function subscribeLanguage(listener: () => void): () => void;
export function setLanguage(language: Language): void;
export function translate(key: string, values?: Record<string, string | number>, language?: Language): string;
export const t: typeof translate;
export function feedback(message: string): string;
export function enumLabel(value: string): string;
