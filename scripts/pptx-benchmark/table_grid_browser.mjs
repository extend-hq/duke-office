import { NormalizedPresentationViewer } from '../../../react-pptx/packages/react-viewer/src/normalized-viewer.ts';
import { collectSlideText } from '../../../react-pptx/packages/presentation-model/src/index.ts';

window.runTableGridProbe = async () => {
  const cases = await (await fetch('./table-grid-models.json')).json();
  const results = [];
  for (const entry of cases) {
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:700px;position:relative';
    document.body.append(host);
    const errors = [];
    const viewer = new NormalizedPresentationViewer(host, entry.model, {
      onNodeError: (id, error) => errors.push({ id, error: String(error) }),
    });
    try {
      await viewer.setZoom(100);
      await viewer.renderSlide(0);
      const cells = [...host.querySelectorAll('td')];
      const node = entry.model.slides[0].nodes.find(n => n.type === 'table');
      const expectedCells = node.rows.flat().filter(c => !c.merge?.horizontal && !c.merge?.vertical);
      const text = cells.map(c => c.textContent);
      const collected = collectSlideText(entry.model.slides[0]);
      const searchHidden = viewer.searchText('Covered').length;
      const searchVisible = viewer.searchText('Visible').length;
      const sameText = JSON.stringify(text) === JSON.stringify(entry.visible);
      const sameSpans = cells.every((cell, i) => cell.colSpan === Math.min(1000, expectedCells[i].colSpan ?? 1) && cell.rowSpan === Math.min(65534, expectedCells[i].rowSpan ?? 1));
      const color = ['rgb(0, 255, 0)', 'rgb(255, 0, 0)', 'rgb(0, 0, 255)', 'rgb(255, 255, 0)'];
      const sameStyle = cells.every((cell, i) => {
        const column = Number(entry.visible[i].split(' ')[2]);
        return cell.style.backgroundColor === color[column];
      });
      const columns = [...host.querySelectorAll('col')].map(c => parseFloat(c.style.width));
      const expectedColumns = [600000, 900000, 1200000, 1500000].map(w => w / 4200000 * 100);
      const sameColumns = columns.length === 4 && columns.every((v, i) => Math.abs(v - expectedColumns[i]) < .0001);
      const row = { id: entry.id, errors, retainedCells: node.rows.flat().length, visibleCells: cells.length,
        coveredCells: entry.covered.length, sameText, sameSpans, sameStyle, sameColumns, searchHidden, searchVisible,
        collectedHidden: collected.includes('Covered'), collectedVisible: entry.visible.every(t => collected.includes(t)) };
      row.passed = !errors.length && sameText && sameSpans && sameStyle && sameColumns && !searchHidden &&
        searchVisible === entry.visible.length && !row.collectedHidden && row.collectedVisible;
      results.push(row);
    } catch (error) {
      results.push({ id: entry.id, passed: false, error: String(error) });
    } finally {
      viewer.destroy(); host.remove();
    }
  }
  window.tableGridResults = results;
  return results;
};
