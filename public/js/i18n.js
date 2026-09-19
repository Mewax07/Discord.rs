/* Runtime i18n partage par la page statique, l'espace compte et la console
   admin. Les dictionnaires vivent dans /js/lang.js, charge avant celui-ci.

   Balisage reconnu :
     data-i18n="cle"                  remplace le texte
     data-i18n-html="cle"             remplace le contenu HTML
     data-i18n-attr="attr:cle|attr2:cle2"  remplace des attributs
     data-v-nom="valeur"              fournit {nom} au gabarit
     data-dur="secondes"              fournit {duration}, mis en forme
     data-lang-switch                 recoit le selecteur de langue

   {site} et {year} viennent de data-site / data-year sur <body>. */

(function () {
    "use strict";

    var LANGS = ["fr", "en", "ru"];
    var FALLBACK = "fr";
    var STORAGE = "bo_lang";
    var DICT = window.BO_LANG || {};

    function detect() {
        var stored = null;
        try {
            stored = localStorage.getItem(STORAGE);
        } catch (error) {
            stored = null;
        }
        if (stored && LANGS.indexOf(stored) >= 0) return stored;

        var candidates = navigator.languages || [navigator.language || ""];
        for (var i = 0; i < candidates.length; i += 1) {
            var code = String(candidates[i]).slice(0, 2).toLowerCase();
            if (LANGS.indexOf(code) >= 0) return code;
        }
        return FALLBACK;
    }

    var lang = detect();
    var listeners = [];

    function raw(key) {
        var table = DICT[lang] || DICT[FALLBACK] || {};
        if (Object.prototype.hasOwnProperty.call(table, key)) return table[key];

        var fallback = DICT[FALLBACK] || {};
        if (Object.prototype.hasOwnProperty.call(fallback, key)) return fallback[key];

        return null;
    }

    /* Francais et anglais : singulier / pluriel. Russe : une forme pour 1,
       une pour 2 a 4, une pour le reste, en excluant 11 a 14. */
    function pluralIndex(count, forms) {
        var index;
        if (lang === "ru") {
            var mod10 = count % 10;
            var mod100 = count % 100;
            if (mod10 === 1 && mod100 !== 11) index = 0;
            else if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) index = 1;
            else index = 2;
        } else {
            index = count === 1 ? 0 : 1;
        }
        return Math.min(index, forms.length - 1);
    }

    function fill(template, vars) {
        if (!vars) return template;
        return template.replace(/\{(\w+)\}/g, function (match, name) {
            return Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : match;
        });
    }

    function t(key, vars) {
        var value = raw(key);
        if (value === null) return key;
        if (!Array.isArray(value)) return fill(value, vars);

        var count = vars && isFinite(vars.count) ? Number(vars.count) : 1;
        return fill(value[pluralIndex(count, value)], vars);
    }

    function list(key) {
        var value = raw(key);
        return Array.isArray(value) ? value : [];
    }

    function duration(seconds) {
        var secs = Number(seconds) || 0;

        var days = Math.floor(secs / 86400);
        if (days >= 1) return t("duration.day", { count: days });

        var hours = Math.floor(secs / 3600);
        if (hours >= 1) return t("duration.hour", { count: hours });

        return t("duration.minute", { count: Math.floor(secs / 60) });
    }

    function globals() {
        var data = document.body ? document.body.dataset : {};
        return { site: data.site || "", year: data.year || "" };
    }

    function varsOf(element) {
        var vars = globals();
        var attributes = element.attributes;

        for (var i = 0; i < attributes.length; i += 1) {
            var name = attributes[i].name;
            if (name.indexOf("data-v-") === 0) vars[name.slice(7)] = attributes[i].value;
        }

        if (element.hasAttribute("data-dur")) vars.duration = duration(element.getAttribute("data-dur"));
        return vars;
    }

    function apply(root) {
        var scope = root || document;

        scope.querySelectorAll("[data-i18n]").forEach(function (element) {
            element.textContent = t(element.getAttribute("data-i18n"), varsOf(element));
        });

        scope.querySelectorAll("[data-i18n-html]").forEach(function (element) {
            element.innerHTML = t(element.getAttribute("data-i18n-html"), varsOf(element));
        });

        scope.querySelectorAll("[data-i18n-attr]").forEach(function (element) {
            var vars = varsOf(element);
            element
                .getAttribute("data-i18n-attr")
                .split("|")
                .forEach(function (pair) {
                    var split = pair.indexOf(":");
                    if (split < 0) return;
                    var attribute = pair.slice(0, split).trim();
                    var key = pair.slice(split + 1).trim();
                    element.setAttribute(attribute, t(key, vars));
                });
        });

        var titleKey = document.body && document.body.dataset.i18nTitle;
        if (!root && titleKey) document.title = t(titleKey, globals());
    }

    function syncSwitch() {
        document.querySelectorAll(".lang-switch button").forEach(function (button) {
            var selected = button.dataset.lang === lang;
            button.classList.toggle("is-active", selected);
            button.setAttribute("aria-pressed", String(selected));
        });
    }

    function mountSwitch() {
        document.querySelectorAll("[data-lang-switch]").forEach(function (host) {
            if (host.querySelector(".lang-switch")) return;

            var group = document.createElement("div");
            group.className = "lang-switch";
            group.setAttribute("role", "group");
            group.setAttribute("aria-label", t("lang.label"));

            LANGS.forEach(function (code) {
                var button = document.createElement("button");
                button.type = "button";
                button.dataset.lang = code;
                button.textContent = t("lang." + code);
                button.addEventListener("click", function () {
                    set(code);
                });
                group.append(button);
            });

            host.append(group);
        });

        syncSwitch();
    }

    function set(next) {
        if (LANGS.indexOf(next) < 0 || next === lang) return;

        lang = next;
        try {
            localStorage.setItem(STORAGE, next);
        } catch (error) {
            /* stockage refuse : la langue tient le temps de la session */
        }

        document.documentElement.lang = next;
        apply();
        syncSwitch();
        listeners.forEach(function (listener) {
            listener(lang);
        });
    }

    document.documentElement.lang = lang;

    window.I18N = {
        get lang() {
            return lang;
        },
        languages: LANGS,
        t: t,
        list: list,
        duration: duration,
        set: set,
        apply: apply,
        onChange: function (listener) {
            listeners.push(listener);
        },
    };

    function boot() {
        apply();
        mountSwitch();
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", boot);
    } else {
        boot();
    }
})();
