const list = document.getElementById("list");
const count = document.getElementById("count");
const loading = document.getElementById("loading");
const menu = document.getElementById("menu");
const nav = document.getElementById("nav");
const toTop = document.getElementById("top");
const shader = document.getElementById("shader");

const HERO_SHADER = {
    lineColor: "#F06FFE",
    glowColor: "#C83DDC",
    speed: 0.2,
    scale: 2,
    rotation: -62,
    rotationSpeed: 0,
    layers: 4,
    waveAmplitude: 0.045,
    waveFrequency: 2.4,
    waveSpeed: 0.4,
    layerSpeed: 0.12,
    twist: 0.1,
    twistFrequency: 0.5,
    twistSpeed: 0.45,
    lineFrequency: 5,
    lineSpacing: 0,
    lineSharpness: 16,
    glowFalloff: 10,
    glowIntensity: 2.9,
    brightness: 2,
    blueBoost: 1.25,
    vignette: 0.68,
    grain: 0.05,
    dpr: 1,
    lightMode: false,
    fps: 60,
    paused: false,
};

const PREVIEW_BASE = {
    speed: 0.16,
    rotationSpeed: 0,
    layers: 3,
    waveAmplitude: 0.045,
    waveFrequency: 2.4,
    waveSpeed: 0.35,
    layerSpeed: 0.1,
    twist: 0.1,
    twistFrequency: 0.5,
    twistSpeed: 0.4,
    lineFrequency: 5,
    lineSpacing: 0,
    lineSharpness: 16,
    glowFalloff: 10,
    glowIntensity: 2.4,
    brightness: 2,
    blueBoost: 1.2,
    vignette: 0.72,
    grain: 0.05,
    dpr: 1,
    fps: 30,
};

const PREVIEW_SHADERS = {
    community: { lineColor: "#1B0E35", glowColor: "#7A34A0", rotation: -34, scale: 1.4 },
    updates: { lineColor: "#F06FFE", glowColor: "#C83DDC", rotation: -62, scale: 1.5 },
    support: { lineColor: "#1B0E35", glowColor: "#7A34A0", rotation: 18, scale: 1.3 },
};

const node = (tag, className, value) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (value) element.textContent = value;
    return element;
};

const faIcon = (name, style) => {
    const element = node("i", `${style || "fa-solid"} ${name}`);
    element.setAttribute("aria-hidden", "true");
    return element;
};

const formatSize = (bytes) => {
    if (typeof bytes !== "number" || bytes <= 0) return null;
    const units = ["o", "Ko", "Mo", "Go"];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
        value /= 1024;
        unit += 1;
    }
    return unit === 0 ? `${value} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
};

const visual = (item) => {
    const wrap = node("div", "download-visual");
    const field = node("div", "download-field");
    field.setAttribute("aria-hidden", "true");
    field.append(node("i"), node("i"), node("i"));
    wrap.append(field);

    if (item.platform) wrap.append(node("span", "download-platform", item.platform));
    wrap.append(faIcon("fa-download"));
    return wrap;
};

const checksum = (hash) => {
    const wrap = node("div", "download-hash");
    wrap.append(node("span", null, "SHA-256"), node("code", null, hash));

    const copy = node("button");
    copy.type = "button";
    copy.title = "Copier l'empreinte";
    copy.setAttribute("aria-label", "Copier l'empreinte SHA-256");
    copy.append(faIcon("fa-copy"));

    copy.addEventListener("click", async () => {
        try {
            await navigator.clipboard.writeText(hash);
            copy.replaceChildren(faIcon("fa-check"));
            setTimeout(() => copy.replaceChildren(faIcon("fa-copy")), 1600);
        } catch (error) {
            console.error("clipboard", error);
        }
    });

    wrap.append(copy);
    return wrap;
};

const card = (item) => {
    const article = node("article", "download-card");
    article.append(visual(item));

    const body = node("div", "download-body");
    body.append(node("h3", null, item.name));

    const meta = [item.version && `v${item.version}`, formatSize(item.size)]
        .filter(Boolean)
        .join(" · ");
    if (meta) body.append(node("p", "download-meta", meta));
    if (item.description) body.append(node("p", null, item.description));
    if (item.sha256) body.append(checksum(item.sha256));

    const available = typeof item.size === "number" && item.size > 0;
    const action = document.createElement("a");
    action.className = available
        ? "primary download-action"
        : "primary download-action disabled";
    action.href = available ? item.url : "#";

    if (available) {
        action.setAttribute("download", "");
        action.append(faIcon("fa-download"), document.createTextNode("Télécharger"));
    } else {
        action.append(faIcon("fa-xmark"), document.createTextNode("Indisponible"));
    }

    body.append(action);
    article.append(body);
    return article;
};

const render = (items) => {
    list.replaceChildren();

    if (!items.length) {
        list.append(node("p", "muted", "Aucune version n'est publiée pour le moment."));
        count.textContent = "";
        return;
    }

    count.textContent =
        items.length === 1 ? "1 version disponible" : `${items.length} versions disponibles`;
    items.forEach((item) => list.append(card(item)));
};

const load = async () => {
    try {
        const response = await fetch("/downloads.json", { cache: "no-store" });
        if (!response.ok) throw new Error(`status ${response.status}`);
        const payload = await response.json();
        render(Array.isArray(payload.items) ? payload.items : []);
    } catch (error) {
        loading?.remove();
        list.replaceChildren(
            node(
                "p",
                "muted",
                "La liste des versions n'a pas pu être chargée. Réessayez dans un instant.",
            ),
        );
        console.error("downloads", error);
    }
};

const mountShaders = () => {
    if (!window.mountGhostFibers) return;

    window.mountGhostFibers(shader, HERO_SHADER);

    document.querySelectorAll(".shader-preview").forEach((preview) => {
        const variant = PREVIEW_SHADERS[preview.dataset.preview];
        if (!variant) return;
        window.mountGhostFibers(preview, Object.assign({}, PREVIEW_BASE, variant));
    });
};

const watchReveal = () => {
    const targets = document.querySelectorAll(".reveal:not(.is-visible)");
    if (!("IntersectionObserver" in window)) {
        targets.forEach((target) => target.classList.add("is-visible"));
        return;
    }

    const observer = new IntersectionObserver(
        (entries) => {
            entries.forEach((entry) => {
                if (!entry.isIntersecting) return;
                entry.target.classList.add("is-visible");
                observer.unobserve(entry.target);
            });
        },
        { rootMargin: "0px 0px -12% 0px", threshold: 0.08 },
    );

    targets.forEach((target) => observer.observe(target));
};

const watchNav = () => {
    const links = [...document.querySelectorAll(".header-inner nav a[href^='#']")];
    const sections = links
        .map((link) => document.querySelector(link.getAttribute("href")))
        .filter(Boolean);

    if (!sections.length || !("IntersectionObserver" in window)) return;

    const observer = new IntersectionObserver(
        (entries) => {
            entries.forEach((entry) => {
                if (!entry.isIntersecting) return;
                links.forEach((link) =>
                    link.classList.toggle(
                        "active",
                        link.getAttribute("href") === `#${entry.target.id}`,
                    ),
                );
            });
        },
        { rootMargin: "-45% 0px -45% 0px" },
    );

    sections.forEach((section) => observer.observe(section));
};

menu?.addEventListener("click", () => {
    const open = nav.classList.toggle("open");
    menu.setAttribute("aria-expanded", String(open));
    menu.replaceChildren(faIcon(open ? "fa-xmark" : "fa-align-justify"));
});

nav?.addEventListener("click", (event) => {
    if (!event.target.closest("a")) return;
    nav.classList.remove("open");
    menu?.setAttribute("aria-expanded", "false");
    menu?.replaceChildren(faIcon("fa-align-justify"));
});

toTop?.addEventListener("click", () => {
    window.scrollTo({ top: 0, behavior: "smooth" });
});

mountShaders();
watchReveal();
watchNav();
load().then(watchReveal);
