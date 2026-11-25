// Halftone browser runtime: boots an app built for wasm32 and paints its cell grid on a canvas.
// Block, braille and box-drawing glyphs are drawn as shapes so they tile without font seams.

const FONT = '"JetBrains Mono", ui-monospace, Menlo, Consolas, monospace';
const FG = '#dfe3e8', BG = '#0a0c0f';
const KEYS = { Enter: 1, Escape: 2, Backspace: 3, Tab: 4, ArrowUp: 6, ArrowDown: 7, ArrowLeft: 8, ArrowRight: 9, Home: 10, End: 11, PageUp: 12, PageDown: 13, Delete: 14 };
const QUAD = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14]; // U+2596..259F as bits UL=1 UR=2 LL=4 LR=8
// U+25xx box drawing: [up, right, down, left], 1 light 2 heavy; 3rd field marks rounded corners.
const BOX = {
  0x00: [0, 1, 0, 1], 0x01: [0, 2, 0, 2], 0x02: [1, 0, 1, 0], 0x03: [2, 0, 2, 0],
  0x0c: [0, 1, 1, 0], 0x0f: [0, 2, 2, 0], 0x10: [0, 0, 1, 1], 0x13: [0, 0, 2, 2],
  0x14: [1, 1, 0, 0], 0x17: [2, 2, 0, 0], 0x18: [1, 0, 0, 1], 0x1b: [2, 0, 0, 2],
  0x1c: [1, 1, 1, 0], 0x24: [1, 0, 1, 1], 0x2c: [0, 1, 1, 1], 0x34: [1, 1, 0, 1], 0x3c: [1, 1, 1, 1],
  0x6d: [0, 1, 1, 0, 1], 0x6e: [0, 0, 1, 1, 1], 0x6f: [1, 0, 0, 1, 1], 0x70: [1, 1, 0, 0, 1],
  0x74: [0, 0, 0, 1], 0x75: [1, 0, 0, 0], 0x76: [0, 1, 0, 0], 0x77: [0, 0, 1, 0],
};

const hex = (c, d) => (c ? '#' + (c & 0xffffff).toString(16).padStart(6, '0') : d);

/** source: URL of app.wasm, or its bytes. opts: { fontSize, mirror: Element, onError(msg), autofocus = true } */
export async function boot(canvas, source, opts = {}) {
  let ex;
  const dec = new TextDecoder();
  const env = { ht_log: (p, n) => fail(dec.decode(new Uint8Array(ex.memory.buffer, p, n))) };
  const bytes = typeof source === 'string' ? await (await fetch(source)).arrayBuffer() : source;
  ex = (await WebAssembly.instantiate(bytes, { env })).instance.exports;

  const size = opts.fontSize || 14;
  await document.fonts.load(`${size}px ${FONT}`).catch(() => {});
  await document.fonts.load(`bold ${size}px ${FONT}`).catch(() => {});

  const g = canvas.getContext('2d', { alpha: false });
  let dpr = 1, cw = 8, ch = 16, cols = 0, rows = 0, prev = null, dead = false;

  function fail(msg) {
    dead = true;
    console.error(msg);
    opts.onError?.(msg);
  }

  function fit(w, h) {
    dpr = window.devicePixelRatio || 1;
    g.font = `${size * dpr}px ${FONT}`;
    cw = Math.round(g.measureText('M').width);
    ch = Math.round(size * dpr * 1.3);
    const c = Math.max(1, Math.floor((w * dpr) / cw)), r = Math.max(1, Math.floor((h * dpr) / ch));
    if (c === cols && r === rows && canvas.width === c * cw) return;
    cols = c; rows = r; prev = null;
    canvas.width = cols * cw; canvas.height = rows * ch;
    canvas.style.width = canvas.width / dpr + 'px';
    canvas.style.height = canvas.height / dpr + 'px';
    if (started) ex.ht_resize(cols, rows);
  }

  function rect(x, y, fx, fy, fw, fh) {
    const x0 = Math.round(x + fx * cw), y0 = Math.round(y + fy * ch);
    g.fillRect(x0, y0, Math.round(x + (fx + fw) * cw) - x0, Math.round(y + (fy + fh) * ch) - y0);
  }

  // Returns false when the glyph should fall back to text.
  function shape(c, x, y) {
    if (c >= 0x2580 && c <= 0x259f) {
      const o = c - 0x2580;
      if (o === 0) rect(x, y, 0, 0, 1, 0.5);
      else if (o <= 8) rect(x, y, 0, 1 - o / 8, 1, o / 8);
      else if (o <= 0xf) rect(x, y, 0, 0, (16 - o) / 8, 1);
      else if (o === 0x10) rect(x, y, 0.5, 0, 0.5, 1);
      else if (o <= 0x13) { g.globalAlpha = (o - 0x10) / 4; rect(x, y, 0, 0, 1, 1); g.globalAlpha = 1; }
      else if (o === 0x14) rect(x, y, 0, 0, 1, 1 / 8);
      else if (o === 0x15) rect(x, y, 7 / 8, 0, 1 / 8, 1);
      else {
        const q = QUAD[o - 0x16];
        if (q & 1) rect(x, y, 0, 0, 0.5, 0.5);
        if (q & 2) rect(x, y, 0.5, 0, 0.5, 0.5);
        if (q & 4) rect(x, y, 0, 0.5, 0.5, 0.5);
        if (q & 8) rect(x, y, 0.5, 0.5, 0.5, 0.5);
      }
      return true;
    }
    if (c >= 0x2800 && c <= 0x28ff) {
      const bits = c - 0x2800, rad = Math.max(0.5, Math.min(cw / 4, ch / 8) * 0.72);
      const pos = [[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2], [0, 3], [1, 3]];
      g.beginPath();
      for (let i = 0; i < 8; i++) {
        if (!(bits & (1 << i))) continue;
        const [dx, dy] = pos[i], px = x + (dx + 0.5) * cw / 2, py = y + (dy + 0.5) * ch / 4;
        g.moveTo(px + rad, py);
        g.arc(px, py, rad, 0, Math.PI * 2);
      }
      g.fill();
      return true;
    }
    const b = c >= 0x2500 && c <= 0x257f && BOX[c - 0x2500];
    if (!b) return false;
    const t = Math.max(1, Math.round(dpr)), mx = x + Math.floor(cw / 2), my = y + Math.floor(ch / 2);
    const w = (k) => t * k;
    if (b[4]) {
      // Rounded corner: one stroked quarter arc between the two edge midpoints.
      g.lineWidth = t;
      g.strokeStyle = g.fillStyle;
      const ax = mx + (t % 2) / 2, ay = my + (t % 2) / 2, r = Math.min(cw, ch) / 2;
      g.beginPath();
      g.moveTo(ax, b[0] ? y : y + ch);
      g.arcTo(ax, ay, b[1] ? x + cw : x, ay, r);
      g.lineTo(b[1] ? x + cw : x, ay);
      g.stroke();
      return true;
    }
    if (b[0]) g.fillRect(mx - (w(b[0]) >> 1), y, w(b[0]), my - y + w(b[0]));
    if (b[2]) g.fillRect(mx - (w(b[2]) >> 1), my - (w(b[2]) >> 1), w(b[2]), y + ch - my + (w(b[2]) >> 1));
    if (b[3]) g.fillRect(x, my - (w(b[3]) >> 1), mx - x + w(b[3]), w(b[3]));
    if (b[1]) g.fillRect(mx - (w(b[1]) >> 1), my - (w(b[1]) >> 1), x + cw - mx + (w(b[1]) >> 1), w(b[1]));
    return true;
  }

  function cell(i, c, f, b, a) {
    const x = (i % cols) * cw, y = Math.floor(i / cols) * ch;
    g.fillStyle = hex(b, BG);
    g.fillRect(x, y, cw, ch);
    if (c === 32) return;
    g.fillStyle = hex(f, FG);
    if (a & 2) g.globalAlpha = 0.55;
    if (!shape(c, x, y)) {
      // shortcut: no per-cell clip, so wide or overhanging glyphs can bleed into a neighbour until it repaints.
      g.font = `${a & 4 ? 'italic ' : ''}${a & 1 ? 'bold ' : ''}${size * dpr}px ${FONT}`;
      g.fillText(String.fromCodePoint(c), x, y + ch / 2 + dpr);
    }
    if (a & 8) g.fillRect(x, y + ch - Math.max(1, Math.round(dpr)) - 1, cw, Math.max(1, Math.round(dpr)));
    g.globalAlpha = 1;
  }

  function paint(p) {
    const n = cols * rows * 4, cur = new Uint32Array(ex.memory.buffer, p, n);
    g.textBaseline = 'middle';
    for (let i = 0; i < n; i += 4) {
      if (prev && prev[i] === cur[i] && prev[i + 1] === cur[i + 1] && prev[i + 2] === cur[i + 2] && prev[i + 3] === cur[i + 3]) continue;
      cell(i >> 2, cur[i], cur[i + 1], cur[i + 2], cur[i + 3]);
    }
    if (!prev || prev.length !== n) prev = new Uint32Array(n);
    prev.set(cur);
  }

  // Screen readers get the grid as plain text, refreshed at most once a second.
  let mirrored = 0;
  function mirror(now) {
    if (!opts.mirror || !prev || now - mirrored < 1000) return;
    mirrored = now;
    let s = '';
    for (let r = 0; r < rows; r++) {
      let line = '';
      for (let c = 0; c < cols; c++) line += String.fromCodePoint(prev[(r * cols + c) * 4] || 32);
      s += line.trimEnd() + '\n';
    }
    if (opts.mirror.textContent !== s) opts.mirror.textContent = s;
  }

  // Input. Pointer coords map straight to cells; capture keeps drags alive outside the canvas.
  const at = (e) => {
    const r = canvas.getBoundingClientRect();
    return [Math.max(0, Math.floor(((e.clientX - r.left) * dpr) / cw)), Math.max(0, Math.floor(((e.clientY - r.top) * dpr) / ch))];
  };
  canvas.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    canvas.focus();
    canvas.setPointerCapture(e.pointerId);
    ex.ht_mouse(0, ...at(e));
    e.preventDefault();
  });
  canvas.addEventListener('pointermove', (e) => ex.ht_mouse(e.buttons & 1 ? 2 : 3, ...at(e)));
  canvas.addEventListener('pointerup', (e) => e.button === 0 && ex.ht_mouse(1, ...at(e)));
  canvas.addEventListener('pointerleave', (e) => !(e.buttons & 1) && ex.ht_mouse(3, 65535, 65535));
  let wheel = 0;
  canvas.addEventListener('wheel', (e) => {
    // Embedded on a scrolling page, the wheel scrolls the page until the app has focus.
    if (document.activeElement !== canvas) return;
    e.preventDefault();
    wheel += e.deltaMode ? e.deltaY * 40 : e.deltaY;
    const [x, y] = at(e);
    for (; wheel <= -40; wheel += 40) ex.ht_mouse(4, x, y);
    for (; wheel >= 40; wheel -= 40) ex.ht_mouse(5, x, y);
  }, { passive: false });

  let escAt = 0;
  canvas.addEventListener('keydown', (e) => {
    if (e.metaKey || e.isComposing) return;
    // Esc then Tab hands focus back to the page, so keyboard users are never trapped.
    if (e.key === 'Tab' && performance.now() - escAt < 1500) return;
    if (e.key === 'Escape') escAt = performance.now();
    const mods = (e.ctrlKey ? 1 : 0) | (e.altKey ? 2 : 0);
    let code = KEYS[e.key];
    if (e.key === 'Tab' && e.shiftKey) code = 5;
    if (code !== undefined) ex.ht_key(code, 0, mods);
    else if ([...e.key].length === 1) ex.ht_key(0, e.key.codePointAt(0), mods);
    else return;
    e.preventDefault();
  });

  let started = false;
  const host = canvas.parentElement;
  const box = () => {
    const s = getComputedStyle(host);
    return [host.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight), host.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom)];
  };
  fit(...box());
  ex.ht_start(cols, rows);
  started = true;
  new ResizeObserver(() => fit(...box())).observe(host);
  matchMedia(`(resolution: ${dpr}dppx)`).addEventListener('change', () => fit(...box()));
  if (opts.autofocus !== false) canvas.focus();

  let last = performance.now();
  function loop(now) {
    if (dead) return;
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    try {
      paint(ex.ht_frame(dt));
    } catch (err) {
      if (!dead) fail(String(err));
      return;
    }
    mirror(now);
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
  return { get cols() { return cols; }, get rows() { return rows; } };
}
