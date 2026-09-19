const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]));
const t = (key, vars) => (window.I18N ? window.I18N.t(key, vars) : key);
let all = [];

function toast(msg) {
  const box = document.getElementById("toast");
  box.textContent = msg;
  box.hidden = false;
  clearTimeout(box._h);
  box._h = setTimeout(() => { box.hidden = true; }, 2800);
}

async function load() {
  const res = await fetch("/admin/data", { headers: { "Accept": "application/json" } });
  if (res.status === 401) { location.reload(); return; }
  const data = await res.json();
  renderStats(data.stats);
  renderVisits(data.visits);
  all = data.licenses || [];
  renderLicenses();
}

function renderStats(s) {
  const cards = [
    { l: "admin.stat.total",    n: s.total },
    { l: "admin.stat.active",   n: s.active,   tone: "mint" },
    { l: "admin.stat.unused",   n: s.unused },
    { l: "admin.stat.expired",  n: s.expired,  tone: "amber" },
    { l: "admin.stat.revoked",  n: s.revoked,  tone: "red" },
    { l: "admin.stat.machines", n: s.machines, tone: "violet" },
  ];
  document.getElementById("stats").innerHTML = cards.map((c) => {
    const tone = c.tone ? ` data-tone="${c.tone}"` : "";
    return `<div class="card"${tone}><div class="n">${esc(c.n)}</div><div class="l" data-i18n="${c.l}">${esc(t(c.l))}</div></div>`;
  }).join("");
}

function renderVisits(v) {
  const box = document.getElementById("visits");
  // Le serveur pose un "Chargement..." traduisible sur le conteneur ; une fois
  // rempli, il doit cesser d'etre une cible du runtime i18n.
  box.removeAttribute("data-i18n");
  box.classList.remove("muted");
  if (!v || v.disabled) {
    box.innerHTML = `<div class="empty"><strong data-i18n="admin.visits.disabledTitle">${esc(t("admin.visits.disabledTitle"))}</strong><span data-i18n="admin.visits.disabledText">${esc(t("admin.visits.disabledText"))}</span></div>`;
    return;
  }
  const days = v.days || {};
  const keys = Object.keys(days).sort().slice(-14);
  const max = Math.max(1, ...keys.map((k) => days[k]));
  const bars = keys.map((k) => {
    const h = Math.max(4, Math.round((days[k] / max) * 100));
    return `<div class="b" style="height:${h}%" data-label="${esc(k)} - ${esc(days[k])}"></div>`;
  }).join("");

  const dls = Object.entries(v.downloads || {}).sort((a, b) => b[1] - a[1]);
  const dlHtml = dls.length
    ? `<div class="dl-list">${dls.map(([id, n]) => `<div class="dl"><code>${esc(id)}</code><strong>${esc(n)}</strong></div>`).join("")}</div>`
    : `<div class="machines-empty" data-i18n="admin.visits.noDownloads">${esc(t("admin.visits.noDownloads"))}</div>`;

  box.innerHTML = `
    <div class="visits-grid">
      <div class="visit-stat"><div class="n">${esc(v.total ?? 0)}</div><div class="l" data-i18n="admin.visits.total">${esc(t("admin.visits.total"))}</div></div>
      <div class="visit-stat"><div class="n">${esc(v.today ?? 0)}</div><div class="l" data-i18n="admin.visits.today">${esc(t("admin.visits.today"))}</div></div>
    </div>
    <div class="subhead"><h3 data-i18n="admin.visits.last14">${esc(t("admin.visits.last14"))}</h3><span class="muted" style="font-size:12px" data-i18n="admin.visits.max" data-v-max="${esc(max)}">${esc(t("admin.visits.max", { max }))}</span></div>
    ${keys.length ? `<div class="bars">${bars}</div>` : `<div class="machines-empty" data-i18n="admin.visits.noData">${esc(t("admin.visits.noData"))}</div>`}
    <div class="subhead"><h3 data-i18n="admin.visits.downloads">${esc(t("admin.visits.downloads"))}</h3></div>
    ${dlHtml}
  `;
}

function statusPill(s) {
  const key = `status.${s}`;
  return `<span class="pill ${esc(s)}"><i></i><span data-i18n="${key}">${esc(t(key))}</span></span>`;
}

function renderLicenses() {
  const q = (document.getElementById("filter").value || "").toLowerCase().trim();
  const rows = all.filter((l) => {
    if (!q) return true;
    return [l.key_prefix, l.product, l.plan, l.owner_id]
      .some((f) => String(f ?? "").toLowerCase().includes(q));
  });

  const host = document.getElementById("licenses");
  host.removeAttribute("data-i18n");
  host.classList.remove("muted");

  if (!rows.length) {
    const title = q ? "admin.noResults" : "admin.noLicenses";
    const text = q ? "admin.noResultsText" : "admin.noLicensesText";
    host.innerHTML = `<div class="empty">
      <svg viewBox="0 0 24 24" fill="none"><path d="M4 7.5 12 3l8 4.5v9L12 21l-8-4.5v-9Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M4 7.5 12 12l8-4.5M12 12v9" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>
      <strong data-i18n="${title}">${esc(t(title))}</strong>
      <span data-i18n="${text}">${esc(t(text))}</span>
    </div>`;
    return;
  }

  const body = rows.map((l) => {
    const machines = (l.activations || []);
    const machinesHtml = machines.length
      ? `<ul class="machines">${machines.map((a) => `<li><span class="dot"></span><code>${esc(a.hwid_short)}</code><span class="muted" data-i18n="admin.machine.meta" data-v-date="${esc(a.last_seen_label)}" data-v-checks="${esc(a.checks)}">${esc(t("admin.machine.meta", { date: a.last_seen_label, checks: a.checks }))}</span></li>`).join("")}</ul>`
      : `<div class="machines-empty" data-i18n="admin.noMachines">${esc(t("admin.noMachines"))}</div>`;

    const owner = l.owner_id
      ? `<code title="${esc(l.owner_id)}">${esc(l.owner_id)}</code>`
      : `<span class="muted" style="font-size:12px" data-i18n="admin.unassigned">${esc(t("admin.unassigned"))}</span>`;

    const expiry = l.lifetime
      ? `<strong data-i18n="admin.lifetime">${esc(t("admin.lifetime"))}</strong>`
      : l.expires_label
        ? `<strong>${esc(l.expires_label)}</strong>`
        : `<strong data-i18n="admin.unknown">${esc(t("admin.unknown"))}</strong>`;

    const toggle = l.revoked
      ? `<button class="btn ghost sm" data-i18n="admin.action.restore" onclick="act('${esc(l.key_prefix)}','restore')">${esc(t("admin.action.restore"))}</button>`
      : `<button class="btn ghost sm" data-i18n="admin.action.revoke" onclick="act('${esc(l.key_prefix)}','revoke')">${esc(t("admin.action.revoke"))}</button>`;

    const actions = `<div class="row-actions">
      ${toggle}
      <button class="btn ghost sm" data-i18n="admin.action.resetHwid" onclick="act('${esc(l.key_prefix)}','reset-hwid')">${esc(t("admin.action.resetHwid"))}</button>
      <button class="btn ghost sm" data-i18n="admin.action.assign" onclick="assign('${esc(l.key_prefix)}')">${esc(t("admin.action.assign"))}</button>
    </div>`;

    return `<tr>
      <td>
        <div class="key-cell">
          <code>${esc(l.key_prefix)}</code>
          ${statusPill(l.status)}
        </div>
      </td>
      <td>
        <div class="product-cell">
          <strong>${esc(l.product)}</strong>
          <span>${esc(l.plan)}</span>
        </div>
      </td>
      <td><div class="owner-cell">${owner}</div></td>
      <td>
        <div class="machine-count">${esc(l.machines_used)} <span>/ ${esc(l.machines_allowed)}</span></div>
        ${machinesHtml}
      </td>
      <td>
        <div class="product-cell">
          ${expiry}
          <span>${esc(l.created_label)}</span>
        </div>
      </td>
      <td>${actions}</td>
    </tr>`;
  }).join("");

  host.innerHTML = `<div class="table-wrap"><table>
    <thead><tr>
      <th data-i18n="admin.table.key">${esc(t("admin.table.key"))}</th>
      <th data-i18n="admin.table.product">${esc(t("admin.table.product"))}</th>
      <th data-i18n="admin.table.owner">${esc(t("admin.table.owner"))}</th>
      <th data-i18n="admin.table.machines">${esc(t("admin.table.machines"))}</th>
      <th data-i18n="admin.table.expiry">${esc(t("admin.table.expiry"))}</th>
      <th data-i18n="admin.table.actions">${esc(t("admin.table.actions"))}</th>
    </tr></thead>
    <tbody>${body}</tbody>
  </table></div>`;
}

async function act(ref, action, extra) {
  if (action === "revoke" && !confirm(t("admin.confirm.revoke"))) return;
  if (action === "reset-hwid" && !confirm(t("admin.confirm.reset"))) return;
  const res = await fetch(`/admin/license/${encodeURIComponent(ref)}/${action}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(extra || {})
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) { toast(data.message || data.error || t("admin.toast.error")); return; }
  const done = {
    "restore": "admin.toast.restored",
    "revoke": "admin.toast.revoked",
    "reset-hwid": "admin.toast.reset",
  };
  toast(t(done[action] || "admin.toast.done"));
  await load();
}

function assign(ref) {
  const owner = prompt(t("admin.prompt.assign"), "");
  if (owner === null) return;
  act(ref, "assign", { owner_id: owner });
}

document.getElementById("filter").addEventListener("input", renderLicenses);
load();
setInterval(load, 30000);
