// The Game Search panel. Talks to the extension through postMessage; the extension talks to
// the server. See editors/code/src/game_search.ts for the message shapes.
// @ts-check
(function () {
  const vscode = acquireVsCodeApi();

  const KINDS = [
    "object",
    "class",
    "embedded",
    "field",
    "hash",
    "link",
    "wad",
    "string",
  ];
  /** Natural, case-insensitive ordering: digits compare by value. */
  const collator = new Intl.Collator(undefined, {
    numeric: true,
    sensitivity: "base",
  });
  const KIND_LABEL = {
    object: "Object",
    class: "Class",
    embedded: "Embedded",
    field: "Field",
    hash: "Hash",
    link: "Link",
    wad: "Wad link",
    string: "String",
  };

  // 16x16 outline icons, drawn with the current colour.
  const ICON = {
    object:
      '<path d="M8 1.5 14 5v6l-6 3.5L2 11V5z"/><path d="M8 8.5 14 5M8 8.5 2 5M8 8.5v6"/>',
    class:
      '<path d="M8 1.5 14 5v6l-6 3.5L2 11V5z"/><path d="M8 8.5 14 5M8 8.5 2 5M8 8.5v6"/>',
    embedded:
      '<path d="M8 3.5 12 5.75v4.5L8 12.5 4 10.25v-4.5z"/><path d="M8 8 12 5.75M8 8 4 5.75M8 8v4.5"/>',
    field:
      '<rect x="2.5" y="4.5" width="11" height="7" rx="1"/><path d="M5 8h6"/>',
    hash: '<circle cx="6" cy="6" r="3.5"/><path d="m8.5 8.5 5 5M11 11l1.5-1.5"/>',
    link: '<path d="M6.5 9.5 9.5 6.5M7 4.5 8.5 3a2.5 2.5 0 0 1 3.5 3.5L10.5 8M5.5 8 4 9.5A2.5 2.5 0 0 0 7.5 13L9 11.5"/>',
    wad: '<path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/>',
    string:
      '<path d="M4.5 3.5c-1.5.5-2 1.5-2 3h2v3h-3v-3c0-2 1-3.5 3-4zM11.5 3.5c-1.5.5-2 1.5-2 3h2v3h-3v-3c0-2 1-3.5 3-4z"/>',
    override: '<path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z"/>',
    chunk:
      '<path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3M6 8h.01M8 8h.01M10 8h.01M6 10.5h.01M8 10.5h.01M10 10.5h.01"/>',
    twist: '<path d="M4 6l4 4 4-4"/>',
    open: '<path d="M6 3H3v10h10v-3M9 3h4v4M13 3 7 9"/>',
    copy: '<rect x="5.5" y="5.5" width="8" height="8" rx="1"/><path d="M10.5 5.5v-3h-8v8h3"/>',
    preview:
      '<path d="M2 8s2.5-4 6-4 6 4 6 4-2.5 4-6 4-6-4-6-4z"/><circle cx="8" cy="8" r="1.8"/>',
    clear: '<path d="M4 4l8 8M12 4l-8 8"/>',
    fold: '<path d="M3 3h10M3 8h10M3 13h10"/><path d="M5.5 5.5 8 8l2.5-2.5M5.5 10.5 8 8l2.5 2.5"/>',
    folder: '<path d="M1.5 3.5h4l1.5 1.5h7.5v8h-13z"/>',
    archive: '<path d="M2 3.5h12v3H2zM3 6.5v7h10v-7M6.5 9h3"/>',
    blank: "",
    refresh:
      '<path d="M13 8a5 5 0 0 1-8.7 3.4M3 8a5 5 0 0 1 8.7-3.4"/><path d="M11 2v3H8M5 14v-3h3"/>',
  };

  // Filled 16x16 icons: the classic folder, and the object cube.
  const FILL = {
    folder:
      '<path d="M1.5 3.5A1 1 0 0 1 2.5 2.5h3.6a1 1 0 0 1 .7.3l1.2 1.2h5.5a1 1 0 0 1 1 1v7.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"/>',
    folderOpen:
      '<path d="M1.5 3.5A1 1 0 0 1 2.5 2.5h3.6a1 1 0 0 1 .7.3l1.2 1.2h5.5a1 1 0 0 1 1 1V6H3.2a1 1 0 0 0-.95.68L1.5 9z"/><path d="M1.6 13.5 3.3 7.4a.6.6 0 0 1 .57-.4h10.9a.4.4 0 0 1 .38.52l-1.6 5.55a.6.6 0 0 1-.58.43z"/>',
    object:
      '<path d="M8 1.2 14.2 4.7v6.6L8 14.8 1.8 11.3V4.7z"/><path fill="rgba(0,0,0,0.28)" d="M8 8.1 14.2 4.7v6.6L8 14.8z"/><path fill="rgba(255,255,255,0.18)" d="M8 8.1 1.8 4.7 8 1.2l6.2 3.5z"/>',
    override: '<path d="M9.2 1.2 3.2 9h4.3l-1 5.8L13 7H8.6z"/>',
  };

  function fill(name, cls) {
    return `<svg class="${cls ?? ""}" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">${FILL[name]}</svg>`;
  }

  function svg(name, cls) {
    return `<svg class="${cls ?? ""}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name]}</svg>`;
  }

  function esc(s) {
    return String(s).replace(
      /[&<>"]/g,
      (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
    );
  }

  /**
   * A path-like name split for layout: the dimmed parent and the last segment that stays
   * whole. A hex hash, or a name with no slash, is all last segment.
   */
  function splitPath(name) {
    const at = name.lastIndexOf("/");
    return at < 0
      ? { pre: "", last: name }
      : { pre: name.slice(0, at), last: name.slice(at + 1) };
  }

  /** A name laid out as dimmed parent, slash, last segment; the parent truncates from the start. */
  function pathHtml(name) {
    const { pre, last } = splitPath(name);
    const parent = pre
      ? `<span class="pre" dir="rtl">&lrm;${highlight(pre, needle)}&lrm;</span><span class="sep">/</span>`
      : "";
    return `${parent}<span class="last">${highlight(last, needle)}</span>`;
  }

  /** `text` with every occurrence of `needle` wrapped in <mark>. */
  function highlight(text, needle) {
    if (!needle || needle.startsWith("0x")) {
      return esc(text);
    }
    const lower = text.toLowerCase();
    let out = "";
    let at = 0;
    let hit = lower.indexOf(needle);
    while (hit >= 0) {
      out +=
        esc(text.slice(at, hit)) +
        "<mark>" +
        esc(text.slice(hit, hit + needle.length)) +
        "</mark>";
      at = hit + needle.length;
      hit = lower.indexOf(needle, at);
    }
    return out + esc(text.slice(at));
  }

  // ---- state ----

  const saved = vscode.getState() ?? {};
  /** @type {{query: string, kinds: string[]}} */
  let current = { query: saved.query ?? "", kinds: saved.kinds ?? [] };
  let indexState = { state: "unconfigured" };
  let seq = 0;
  /** The last result set: hits in order, plus facets. */
  let hits = [];
  let total = 0;
  let facets = [];
  let needle = "";
  let loading = false;
  let selected = -1;
  /** Preview text per "chunk|object" key, or "loading". */
  const previews = new Map();
  const openPreviews = new Set();
  const closedGroups = new Set();
  /** Rows whose folded matched terms are shown, by "chunk|object" key. */
  const openTerms = new Set();
  /** Parent-path folders the user closed, by "group\0section\0parent" key. */
  const closedFolders = new Set();
  let allFolded = false;

  /** Matched terms shown inline before the rest fold behind "+N". */
  const INLINE_TERMS = 2;

  // ---- browse tree (shown while the query is empty) ----

  /** Listings by key: "root", "d:<archive>\0<dir>", "c:<hash>". A value is a listing, "loading" or {error}. */
  const listings = new Map();
  const treeOpen = new Set();

  function listingKey(params) {
    return "d:" + (params.dir ?? "");
  }

  function requestListing(params) {
    const key = listingKey(params);
    if (!listings.has(key)) {
      listings.set(key, "loading");
      vscode.postMessage({ type: "browse", key, params });
    }
    return listings.get(key);
  }

  // ---- elements ----

  const $ = (sel) => /** @type {HTMLElement} */ (document.querySelector(sel));
  const input = /** @type {HTMLInputElement} */ (
    document.getElementById("query")
  );
  const chips = $("#chips");
  const status = $("#status");
  const results = $("#results");
  $("#clear").innerHTML = svg("clear");
  $("#fold").innerHTML = svg("fold");
  $("#rebuild").innerHTML = svg("refresh");
  input.value = current.query;

  // ---- rendering ----

  function renderChips() {
    chips.innerHTML = KINDS.map((kind) => {
      const count = facets.find((f) => f.kind === kind)?.count;
      const on = current.kinds.includes(kind);
      const empty = count === 0 && !on;
      return `<button class="chip ${on ? "on" : ""} ${empty ? "empty" : ""}" data-kind="${kind}" title="${KIND_LABEL[kind]} matches">${svg(kind)}<span>${KIND_LABEL[kind]}</span>${count !== undefined ? `<span class="n">${count.toLocaleString()}</span>` : ""}</button>`;
    }).join("");
  }

  function renderStatus() {
    status.classList.remove("error");
    const s = indexState;
    if (s.state === "building") {
      const stage =
        s.stage === "walk"
          ? `Indexing archives ${s.done}/${s.total}`
          : `Indexing (${s.stage})`;
      const pct =
        s.stage === "walk" && s.total
          ? Math.round((100 * s.done) / s.total)
          : null;
      status.innerHTML = `${esc(stage)}${pct !== null ? `<progress max="100" value="${pct}"></progress>` : ""}`;
      return;
    }
    if (s.state === "failed") {
      status.classList.add("error");
      status.textContent = `Index failed: ${s.message}`;
      return;
    }
    if (s.state === "unconfigured") {
      status.innerHTML = `No game directory. <a href="#" id="settings">Set ritobin-lsp.gameDirectory</a>`;
      return;
    }
    if (loading) {
      status.textContent = "Searching…";
      return;
    }
    if (!current.query.trim()) {
      status.textContent = `${s.declarations.toLocaleString()} objects in ${s.bins.toLocaleString()} bins`;
      return;
    }
    const shown = hits.length;
    status.textContent =
      total === 0
        ? "No results."
        : shown < total
          ? `${total.toLocaleString()} objects, showing ${shown.toLocaleString()}`
          : `${total.toLocaleString()} object${total === 1 ? "" : "s"}`;
  }

  function primaryKind(hit) {
    if (hit.isOverride) return "override";
    return hit.matches[0]?.kind ?? "object";
  }

  function renderResults() {
    if (indexState.state !== "ready" && hits.length === 0) {
      results.innerHTML = "";
      return;
    }
    if (!current.query.trim()) {
      renderTree();
      return;
    }
    if (hits.length === 0) {
      results.innerHTML = loading
        ? ""
        : `<div class="welcome">Nothing matches "${esc(current.query)}"${current.kinds.length ? " in the selected kinds" : ""}.</div>`;
      return;
    }
    // chunk > (kind section when mixed) > parent path > object, in server order.
    const groups = [];
    const byKey = new Map();
    hits.forEach((hit, i) => {
      const key = hit.archive + "\0" + hit.chunk.hash;
      let g = byKey.get(key);
      if (!g) {
        g = { key, archive: hit.archive, chunk: hit.chunk, rows: [] };
        byKey.set(key, g);
        groups.push(g);
      }
      g.rows.push(i);
    });
    const html = groups.map((g) => {
      const closed = closedGroups.has(g.key);
      const chunkName = g.chunk.name ?? g.chunk.hash;
      const body = renderFolders(g.key, g.rows);
      return `<div class="group ${closed ? "closed" : ""}" data-group="${esc(g.key)}">
        <div class="group-head" title="${esc(chunkName)} in ${esc(g.archive)}">
          ${svg("twist", "twist")}${svg("chunk", "kind")}
          <span class="chunk path">${pathHtml(chunkName)}</span>
          <span class="archive">${esc(g.archive)}</span>
          <span class="count">${g.rows.length}</span>
        </div>
        <div class="rows">${body}</div>
      </div>`;
    });
    if (hits.length < total) {
      const left = total - hits.length;
      html.push(
        `<div class="more-row" id="more"><button id="load-more">Load ${Math.min(500, left).toLocaleString()} more</button><span>${left.toLocaleString()} not shown</span></div>`,
      );
    }
    results.innerHTML = html.join("");
    observeMore();
  }

  /** The installation as the tree of its object paths. */
  function renderTree() {
    try {
      results.innerHTML = `<div class="tree">${renderDir("")}</div>`;
    } catch (e) {
      status.classList.add("error");
      status.textContent = `The tree failed to render: ${e && e.message ? e.message : e}`;
    }
  }

  function objectAttrs(o) {
    return `data-chunk="${esc(o.chunk.hash)}" data-path="${esc(o.chunk.name ?? "")}" data-object="${esc(o.object.hash)}"`;
  }

  function objectTitle(o) {
    const cls = o.class.name ?? o.class.hash;
    const chunk = o.chunk.name ?? o.chunk.hash;
    return `${o.object.name ?? o.object.hash}\n${cls}\nobject ${o.object.hash}\nchunk ${chunk} in ${o.archive}${o.isOverride ? " (PTCH override)" : ""}`;
  }

  function renderDir(dir) {
    const listing = requestListing({ dir });
    if (listing === "loading") return `<div class="tnote">Loading…</div>`;
    if (listing.error) return `<div class="tnote">${esc(listing.error)}</div>`;
    // Folders first, then objects, each in natural order: Skin2 before Skin10.
    const entries = [...listing.entries].sort(
      (a, b) =>
        (a.kind === "dir" ? 0 : 1) - (b.kind === "dir" ? 0 : 1) ||
        collator.compare(a.name, b.name),
    );
    const rows = entries.map((e) => {
      if (e.kind === "dir") {
        const sub = dir ? `${dir}/${e.name}` : e.name;
        const key = "d:" + sub;
        const open = treeOpen.has(key);
        const own = e.object;
        const cls = own
          ? `<span class="class">${esc(own.class.name ?? own.class.hash)}</span>`
          : "";
        const actions = own
          ? `<span class="actions"><button class="icon-button" data-act="open-object" title="Open object">${svg("open")}</button></span>`
          : "";
        return `<div class="tnode ${open ? "" : "closed"}" data-key="${esc(key)}">
          <div class="trow dir ${own ? "has-object" : ""}" ${own ? objectAttrs(own) : ""} title="${esc(own ? objectTitle(own) : sub)}">${svg("twist", "twist")}${fill(open ? "folderOpen" : "folder", "kind folder")}<span class="name">${esc(e.name)}</span>${cls}<span class="meta">${e.objects.toLocaleString()}</span>${actions}</div>
          <div class="children">${open ? renderDir(sub) : ""}</div>
        </div>`;
      }
      const kind = e.isOverride ? "override" : "object";
      const chunk = e.chunk.name ?? e.chunk.hash;
      return `<div class="tnode leaf"><div class="trow object" ${objectAttrs(e)} title="${esc(objectTitle(e))}">${svg("blank", "twist")}${fill(kind, `kind ${kind}`)}<span class="name">${esc(e.name)}</span><span class="class">${esc(e.class.name ?? e.class.hash)}</span><span class="meta">${esc(chunk.split("/").pop() ?? chunk)}</span></div></div>`;
    });
    if (listing.truncated) {
      rows.push(
        `<div class="tnote">${(listing.total - listing.entries.length).toLocaleString()} more not listed; search reaches them.</div>`,
      );
    }
    return rows.join("");
  }

  /** Rows of one section folded under their parent object path, in order of first appearance. */
  function renderFolders(prefix, idx) {
    const folders = [];
    const byParent = new Map();
    for (const i of idx) {
      const { pre } = splitPath(hits[i].object.name ?? hits[i].object.hash);
      let f = byParent.get(pre);
      if (!f) {
        f = { parent: pre, rows: [] };
        byParent.set(pre, f);
        folders.push(f);
      }
      f.rows.push(i);
    }
    return folders
      .map((f) => {
        const rows = f.rows.map((i) => renderRow(hits[i], i)).join("");
        if (!f.parent) {
          return rows;
        }
        const key = prefix + "\0" + f.parent;
        const closed = closedFolders.has(key);
        return `<div class="folder ${closed ? "closed" : ""}" data-folder="${esc(key)}">
          <div class="folder-head" title="${esc(f.parent)}">${svg("twist", "twist")}${fill(closed ? "folder" : "folderOpen", "kind folder")}<span class="parent" dir="rtl">&lrm;${highlight(f.parent, needle)}/&lrm;</span><span class="n">${f.rows.length}</span></div>
          <div class="rows">${rows}</div>
        </div>`;
      })
      .join("");
  }

  function renderRow(hit, i) {
    const kind = primaryKind(hit);
    const name = hit.object.name ?? hit.object.hash;
    const { last } = splitPath(name);
    const cls = hit.class.name ?? hit.class.hash;
    const key = hit.chunk.hash + "|" + hit.object.hash;
    // Matched terms other than the object's own path and class, minus any that merely repeat
    // the object's path.
    const lowerName = name.toLowerCase();
    const terms = hit.matches.filter(
      (m) =>
        m.kind !== "object" &&
        m.kind !== "class" &&
        m.text.toLowerCase() !== lowerName,
    );
    const termsOpen = openTerms.has(key);
    let sub = "";
    let termList = "";
    if (terms.length) {
      if (termsOpen) {
        // The whole list under the object, one term per line.
        termList = `<div class="terms">${terms
          .map(
            (m) =>
              `<div class="term"><span class="t ${m.kind}">${highlight(m.text, needle)}</span><span class="tk">${esc(KIND_LABEL[m.kind])}</span></div>`,
          )
          .join(
            "",
          )}<button class="fold-terms" data-act="terms">${svg("twist", "twist open")}show less</button></div>`;
      } else {
        const items = terms
          .slice(0, INLINE_TERMS)
          .map(
            (m) =>
              `<span class="t ${m.kind}" title="${esc(KIND_LABEL[m.kind])}">${highlight(m.text, needle)}</span>`,
          )
          .join("");
        const fold =
          terms.length > INLINE_TERMS
            ? `<button class="fold-terms" data-act="terms" title="Every matched term, listed under the object">${svg("twist", "twist")}+${terms.length - INLINE_TERMS}</button>`
            : "";
        sub = `<div class="sub">${items}${fold}</div>`;
      }
    }
    const open = openPreviews.has(key);
    const title = `${name}\n${cls}\nobject ${hit.object.hash}\nchunk ${hit.chunk.name ?? hit.chunk.hash}${hit.isOverride ? " (PTCH override)" : ""}`;
    let preview = "";
    if (open) {
      const p = previews.get(key);
      preview = `<div class="preview" data-key="${esc(key)}">${p === undefined || p === "loading" ? '<span class="more">Loading…</span>' : p === null ? '<span class="more">No preview.</span>' : esc(p.text) + (p.truncated ? '\n<span class="more">…</span>' : "")}</div>`;
    }
    return `<div class="row ${selected === i ? "selected" : ""} ${sub ? "two" : ""}" data-i="${i}" title="${esc(title)}">
      <div class="line">
        ${kind === "object" || kind === "class" || kind === "override" ? fill(kind === "override" ? "override" : "object", `kind ${kind}`) : svg(kind, `kind ${kind}`)}
        <span class="last">${highlight(last, needle)}</span>
        <span class="class">${highlight(cls, needle)}</span>
        <span class="actions">
          <button class="icon-button" data-act="preview" title="Preview (Space)">${svg("preview")}</button>
          <button class="icon-button" data-act="copy-name" title="Copy name">${svg("copy")}</button>
          <button class="icon-button" data-act="open" title="Open (Enter)">${svg("open")}</button>
        </span>
      </div>
      ${sub}
    </div>${termList}${preview}`;
  }

  let observer = null;
  function observeMore() {
    observer?.disconnect();
    const more = document.getElementById("more");
    if (!more) return;
    observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) loadMore();
    });
    observer.observe(more);
  }

  function renderAll() {
    renderChips();
    renderStatus();
    try {
      renderResults();
    } catch (e) {
      status.classList.add("error");
      status.textContent = `Rendering failed: ${e && e.message ? e.message : e}`;
    }
  }

  // ---- searching ----

  let debounce = 0;
  function scheduleSearch() {
    clearTimeout(debounce);
    debounce = setTimeout(() => runSearch(0), 180);
  }

  function runSearch(offset) {
    const query = input.value;
    current = { query, kinds: current.kinds };
    vscode.setState({ query, kinds: current.kinds });
    vscode.postMessage({ type: "persist", query, kinds: current.kinds });
    needle = query.trim().toLowerCase();
    if (!needle) {
      hits = [];
      total = 0;
      facets = [];
      loading = false;
      renderAll();
      return;
    }
    if (indexState.state !== "ready") {
      return;
    }
    if (offset === 0) {
      selected = -1;
      openPreviews.clear();
    }
    loading = true;
    seq += 1;
    vscode.postMessage({
      type: "search",
      seq,
      query,
      kinds: current.kinds,
      offset,
    });
    renderStatus();
  }

  function loadMore() {
    if (loading || hits.length >= total) return;
    runSearch(hits.length);
  }

  // ---- actions ----

  function hitAt(i) {
    return hits[i];
  }

  function open(hit) {
    vscode.postMessage({
      type: "open",
      chunk: hit.chunk.hash,
      display: hit.chunk.name ?? null,
      object: hit.object.hash,
    });
  }

  function togglePreview(i) {
    const hit = hitAt(i);
    if (!hit) return;
    const key = hit.chunk.hash + "|" + hit.object.hash;
    if (openPreviews.has(key)) {
      openPreviews.delete(key);
    } else {
      openPreviews.add(key);
      if (!previews.has(key)) {
        previews.set(key, "loading");
        vscode.postMessage({
          type: "preview",
          key,
          chunk: hit.chunk.hash,
          object: hit.object.hash,
        });
      }
    }
    renderResults();
  }

  /** Moves the selection to `i`, stepping over rows hidden in a folded section or group. */
  function select(i, scroll) {
    const step = i >= selected ? 1 : -1;
    let j = Math.max(-1, Math.min(hits.length - 1, i));
    while (j >= 0 && j < hits.length) {
      const el = results.querySelector(`.row[data-i="${j}"]`);
      if (el && el.offsetParent !== null) break;
      j += step;
    }
    if (j < 0 || j >= hits.length) return;
    selected = j;
    results
      .querySelectorAll(".row.selected")
      .forEach((el) => el.classList.remove("selected"));
    const row = results.querySelector(`.row[data-i="${selected}"]`);
    if (row) {
      row.classList.add("selected");
      if (scroll) row.scrollIntoView({ block: "nearest" });
    }
  }

  // ---- events ----

  input.addEventListener("input", scheduleSearch);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      clearTimeout(debounce);
      runSearch(0);
    } else if (e.key === "ArrowDown" && hits.length) {
      e.preventDefault();
      results.focus();
      select(0, true);
    } else if (e.key === "Escape") {
      input.value = "";
      runSearch(0);
    }
  });

  $("#clear").addEventListener("click", () => {
    input.value = "";
    input.focus();
    runSearch(0);
  });
  $("#rebuild").addEventListener("click", () =>
    vscode.postMessage({ type: "rebuild" }),
  );
  $("#fold").addEventListener("click", () => {
    allFolded = !allFolded;
    closedGroups.clear();
    if (allFolded) {
      results
        .querySelectorAll(".group")
        .forEach((g) => closedGroups.add(g.dataset.group));
    }
    renderResults();
  });

  chips.addEventListener("click", (e) => {
    const chip = /** @type {HTMLElement} */ (e.target).closest(".chip");
    if (!chip) return;
    const kind = chip.dataset.kind;
    current.kinds = current.kinds.includes(kind)
      ? current.kinds.filter((k) => k !== kind)
      : [...current.kinds, kind];
    renderChips();
    runSearch(0);
  });

  status.addEventListener("click", (e) => {
    if (/** @type {HTMLElement} */ (e.target).id === "settings") {
      e.preventDefault();
      vscode.postMessage({ type: "openSettings" });
    }
  });

  results.addEventListener("click", (e) => {
    const target = /** @type {HTMLElement} */ (e.target);
    const act = target.closest("[data-act]");
    const trow = target.closest(".trow");
    if (trow) {
      e.stopPropagation();
      const node = trow.parentElement;
      const opensObject =
        trow.classList.contains("object") ||
        (trow.classList.contains("has-object") &&
          act &&
          act.dataset.act === "open-object");
      if (opensObject) {
        vscode.postMessage({
          type: "open",
          chunk: trow.dataset.chunk,
          display: trow.dataset.path || null,
          object: trow.dataset.object,
        });
        return;
      }
      if (!node.dataset.key) return;
      const key = node.dataset.key;
      if (treeOpen.has(key)) treeOpen.delete(key);
      else treeOpen.add(key);
      renderTree();
      return;
    }
    const row = target.closest(".row");
    if (act && row) {
      e.stopPropagation();
      const i = Number(row.dataset.i);
      const hit = hitAt(i);
      if (!hit) return;
      switch (act.dataset.act) {
        case "terms": {
          const k = hit.chunk.hash + "|" + hit.object.hash;
          if (openTerms.has(k)) openTerms.delete(k);
          else openTerms.add(k);
          renderResults();
          break;
        }
        case "preview":
          togglePreview(i);
          break;
        case "copy-name":
          vscode.postMessage({
            type: "copy",
            text: hit.object.name ?? hit.object.hash,
          });
          break;
        case "open":
          open(hit);
          break;
      }
      return;
    }
    if (row) {
      const i = Number(row.dataset.i);
      select(i, false);
      open(hitAt(i));
      return;
    }
    const folderHead = target.closest(".folder-head");
    if (folderHead) {
      const folder = folderHead.parentElement;
      const key = folder.dataset.folder;
      if (closedFolders.has(key)) closedFolders.delete(key);
      else closedFolders.add(key);
      folder.classList.toggle("closed");
      return;
    }
    const head = target.closest(".group-head");
    if (head) {
      const group = head.parentElement;
      const key = group.dataset.group;
      if (closedGroups.has(key)) closedGroups.delete(key);
      else closedGroups.add(key);
      group.classList.toggle("closed");
      return;
    }
    if (target.id === "load-more") {
      loadMore();
    }
  });

  // A folder that is itself an object opens on double click; a single click folds it.
  results.addEventListener("dblclick", (e) => {
    const trow = /** @type {HTMLElement} */ (e.target).closest(
      ".trow.has-object",
    );
    if (trow) {
      vscode.postMessage({
        type: "open",
        chunk: trow.dataset.chunk,
        display: trow.dataset.path || null,
        object: trow.dataset.object,
      });
    }
  });

  results.addEventListener("keydown", (e) => {
    if (!hits.length) return;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        select(selected + 1, true);
        break;
      case "ArrowUp":
        e.preventDefault();
        if (selected <= 0) {
          select(-1, false);
          input.focus();
        } else {
          select(selected - 1, true);
        }
        break;
      case "Enter":
        e.preventDefault();
        if (selected >= 0) open(hitAt(selected));
        break;
      case " ":
        e.preventDefault();
        if (selected >= 0) togglePreview(selected);
        break;
      case "Escape":
        input.focus();
        break;
      case "c":
        if ((e.ctrlKey || e.metaKey) && selected >= 0) {
          e.preventDefault();
          const hit = hitAt(selected);
          vscode.postMessage({
            type: "copy",
            text: hit.object.name ?? hit.object.hash,
          });
        }
        break;
    }
  });
  results.tabIndex = 0;

  window.addEventListener("message", (e) => {
    const msg = e.data;
    switch (msg.type) {
      case "init":
        indexState = msg.state;
        if (msg.lastQuery && !current.query) {
          current = msg.lastQuery;
          input.value = current.query;
        }
        renderAll();
        if (current.query.trim() && indexState.state === "ready") runSearch(0);
        break;
      case "indexState": {
        const wasReady = indexState.state === "ready";
        if (msg.state.state !== "ready") {
          listings.clear();
        }
        indexState = msg.state;
        renderAll();
        if (!wasReady && indexState.state === "ready" && current.query.trim())
          runSearch(0);
        break;
      }
      case "results":
        if (msg.seq !== seq) return;
        loading = false;
        if (msg.append) {
          hits = hits.concat(msg.result.hits);
        } else {
          hits = msg.result.hits;
          results.scrollTop = 0;
        }
        total = msg.result.total;
        facets = msg.result.kinds;
        renderAll();
        break;
      case "error":
        loading = false;
        status.classList.add("error");
        status.textContent = msg.message;
        break;
      case "preview":
        previews.set(msg.key, msg.preview ?? null);
        if (openPreviews.has(msg.key)) renderResults();
        break;
      case "listing":
        listings.set(
          msg.key,
          msg.listing ?? { error: msg.error ?? "no listing" },
        );
        if (!current.query.trim()) renderTree();
        break;
      case "focus":
        if (typeof msg.query === "string") {
          input.value = msg.query;
          runSearch(0);
        }
        input.focus();
        input.select();
        break;
    }
  });

  renderAll();
  vscode.postMessage({ type: "ready" });
})();
