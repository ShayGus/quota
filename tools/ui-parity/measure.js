// Computed-style measurement, evaluated in both the wireframe page and the live
// app webview. It reads globalThis.__paritySelectors and __parityScope and
// returns one row of sizes and computed properties per selector. Text content
// is deliberately not returned, so results never carry account data.
(() => {
  const props = [
    "font-size",
    "font-weight",
    "line-height",
    "letter-spacing",
    "color",
    "background-color",
    "padding",
    "margin",
    "border-radius",
    "border-top-color",
    "border-top-width",
    "gap",
    "text-transform",
    "opacity",
    "box-shadow",
  ];
  const selectors = globalThis.__paritySelectors ?? [];
  const scope = globalThis.__parityScope
    ? document.querySelector(globalThis.__parityScope)
    : document;
  const out = {};
  for (const selector of selectors) {
    const element = scope?.querySelector(selector);
    if (!element) {
      out[selector] = null;
      continue;
    }
    const style = getComputedStyle(element);
    const box = element.getBoundingClientRect();
    const row = {
      w: Math.round(box.width * 10) / 10,
      h: Math.round(box.height * 10) / 10,
    };
    for (const prop of props) row[prop] = style.getPropertyValue(prop);
    out[selector] = row;
  }
  return out;
})();
