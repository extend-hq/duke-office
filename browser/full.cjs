exports.loadOfficeCharts = (source) =>
  import("./full.js").then(({ loadOfficeCharts }) => loadOfficeCharts(source));
