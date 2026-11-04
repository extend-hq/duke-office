exports.loadOfficeCharts = (source) =>
  import("./index.js").then(({ loadOfficeCharts }) => loadOfficeCharts(source));
