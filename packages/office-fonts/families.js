export const families = [
  { id: "carlito", family: "Carlito", original: "Calibri", load: () => import("./carlito-data.js") },
  { id: "arimo", family: "Arimo", original: "Arial", load: () => import("./arimo-data.js") },
  { id: "tinos", family: "Tinos", original: "Times New Roman", load: () => import("./tinos-data.js") },
  { id: "cousine", family: "Cousine", original: "Courier New", load: () => import("./cousine-data.js") }
];
