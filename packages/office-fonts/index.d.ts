export const OFFICE_FALLBACK_FONT: "Carlito";
export function loadOfficeFonts(target?: Document, families?: Iterable<string>): Promise<void>;
export function officeFontFallback(family: string): string | undefined;
export function embedOfficeFonts(svg: string): Promise<string>;
