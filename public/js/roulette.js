(function () {
    "use strict";

    var root = document.getElementById("roulette");
    if (!root) return;

    var form = document.getElementById("spin-form");
    var button = document.getElementById("spin-btn");
    var reel = document.getElementById("reel");
    var strip = document.getElementById("strip");
    var result = document.getElementById("result");
    var hint = document.getElementById("spin-hint");
    var pool = JSON.parse(root.dataset.pool);

    var TILE = 148;
    var GAP = 10;
    var STRIDE = TILE + GAP;
    var LENGTH = 64;
    var WIN_INDEX = 52;
    var busy = false;

    var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    function t(key, vars) {
        return window.I18N ? window.I18N.t(key, vars) : key;
    }

    function rand(n) {
        return Math.floor(Math.random() * n);
    }

    function pick(list) {
        return list[rand(list.length)];
    }

    function fillerKind() {
        var entries = Object.keys(pool.weights).filter(function (k) {
            return pool.weights[k] > 0;
        });
        if (!entries.length) return "nothing";
        var total = entries.reduce(function (sum, k) {
            return sum + pool.weights[k];
        }, 0);
        var roll = Math.random() * total;
        for (var i = 0; i < entries.length; i += 1) {
            roll -= pool.weights[entries[i]];
            if (roll < 0) return entries[i];
        }
        return entries[0];
    }

    function fillerTile() {
        var kind = fillerKind();
        if (kind === "coins") {
            var span = pool.coins[1] - pool.coins[0] + 1;
            return { kind: kind, amount: pool.coins[0] + rand(Math.max(span, 1)) };
        }
        if (kind === "license") return { kind: kind, days: pick(pool.licenses) };
        if (kind === "emote") return { kind: kind, name: pick(pool.emotes) };
        if (kind === "cosmetic") return { kind: kind, name: pick(pool.cosmetics) };
        return { kind: "nothing" };
    }

    function label(tile) {
        if (tile.kind === "nothing") return t("account.roulette.tile.nothing");
        if (tile.kind === "coins") return t("account.roulette.tile.coins", { amount: tile.amount });
        if (tile.kind === "license") return t("account.roulette.tile.license", { days: tile.days });
        return tile.name;
    }

    function buildTile(tile, winner) {
        var node = document.createElement("div");
        node.className = "reel-tile kind-" + tile.kind + (winner ? " is-winner" : "");
        var kind = document.createElement("span");
        kind.className = "reel-tile-kind";
        kind.textContent = t("account.roulette.kind." + tile.kind);
        var name = document.createElement("strong");
        name.textContent = label(tile);
        node.appendChild(kind);
        node.appendChild(name);
        return node;
    }

    function render(tiles) {
        strip.textContent = "";
        tiles.forEach(function (tile, index) {
            strip.appendChild(buildTile(tile, index === WIN_INDEX && tile.winner));
        });
    }

    function idle() {
        var tiles = [];
        for (var i = 0; i < LENGTH; i += 1) tiles.push(fillerTile());
        render(tiles);
        strip.style.transition = "none";
        strip.style.transform = "translateX(" + -(WIN_INDEX * STRIDE - reel.clientWidth / 2 + TILE / 2) + "px)";
    }

    function play(outcome) {
        var tiles = [];
        for (var i = 0; i < LENGTH; i += 1) tiles.push(fillerTile());
        var win = Object.assign({}, outcome, { winner: true });
        tiles[WIN_INDEX] = win;

        render(tiles);
        strip.style.transition = "none";
        strip.style.transform = "translateX(0)";
        void strip.offsetWidth;

        var duration = reduced ? 400 : 7200;
        var jitter = (Math.random() * 0.7 + 0.15) * TILE;
        var target = WIN_INDEX * STRIDE + jitter - reel.clientWidth / 2;

        strip.style.transition = "transform " + duration + "ms cubic-bezier(0.08, 0.62, 0.1, 1)";
        strip.style.transform = "translateX(" + -target + "px)";
        reel.classList.add("is-spinning");

        return new Promise(function (resolve) {
            var done = false;
            function finish() {
                if (done) return;
                done = true;
                reel.classList.remove("is-spinning");
                reel.classList.add("has-result");
                resolve();
            }
            strip.addEventListener("transitionend", finish, { once: true });
            setTimeout(finish, duration + 300);
        });
    }

    function showResult(outcome) {
        var text = document.createElement("p");
        var key = "account.roulette.outcome." + outcome.kind;
        text.setAttribute("data-i18n", key);
        if (outcome.amount !== undefined) text.setAttribute("data-v-amount", outcome.amount);
        if (outcome.name !== undefined) text.setAttribute("data-v-name", outcome.name);
        if (outcome.days !== undefined) text.setAttribute("data-v-days", outcome.days);
        if (outcome.key !== undefined) text.setAttribute("data-v-key", outcome.key);
        result.textContent = "";
        result.className = "reel-result kind-" + outcome.kind;
        result.appendChild(text);
        result.hidden = false;
        if (window.I18N) window.I18N.apply(result);
    }

    function updateStats(data) {
        document.getElementById("stat-coins").textContent = data.coins;
        document.getElementById("stat-spins").textContent = data.spins;

        var free = document.getElementById("stat-free");
        var span = document.createElement("span");
        if (data.free_in > 0) {
            span.setAttribute("data-i18n", "account.roulette.freeIn");
            span.setAttribute("data-dur", data.free_in);
        } else {
            span.setAttribute("data-i18n", "account.roulette.freeNow");
        }
        free.textContent = "";
        free.appendChild(span);

        hint.textContent = "";
        if (!data.can_spin) {
            var note = document.createElement("span");
            note.setAttribute("data-i18n", "account.roulette.cantSpin");
            hint.appendChild(note);
        }
        button.disabled = !data.can_spin;
        if (window.I18N) window.I18N.apply(document.querySelector(".wrap"));
    }

    function addHistory(outcome) {
        if (outcome.kind !== "emote" && outcome.kind !== "cosmetic") return;
        var list = document.getElementById("history");
        var empty = document.getElementById("history-empty");
        var item = document.createElement("li");
        item.className = "prize kind-" + outcome.kind;
        var kind = document.createElement("span");
        kind.className = "prize-kind";
        kind.setAttribute("data-i18n", "account.roulette.kind." + outcome.kind);
        var name = document.createElement("strong");
        name.textContent = outcome.name;
        item.appendChild(kind);
        item.appendChild(name);
        list.insertBefore(item, list.firstChild);
        empty.hidden = true;
        if (window.I18N) window.I18N.apply(item);
    }

    function fail(reason) {
        var key = reason === "hwid" ? "account.roulette.needHwid" : reason === "funds" ? "account.roulette.cantSpin" : "account.roulette.error";
        hint.textContent = "";
        var note = document.createElement("span");
        note.setAttribute("data-i18n", key);
        hint.appendChild(note);
        if (window.I18N) window.I18N.apply(hint);
    }

    form.addEventListener("submit", function (event) {
        event.preventDefault();
        if (busy || button.disabled) return;
        busy = true;
        button.disabled = true;
        result.hidden = true;
        reel.classList.remove("has-result");

        fetch(form.action, {
            method: "POST",
            credentials: "same-origin",
            headers: { Accept: "application/json" },
        })
            .then(function (response) {
                return response.json().catch(function () {
                    return { ok: false, reason: "error" };
                });
            })
            .then(function (data) {
                if (!data.ok) {
                    fail(data.reason);
                    button.disabled = data.reason === "hwid" || data.reason === "funds";
                    busy = false;
                    return;
                }
                return play(data.outcome).then(function () {
                    showResult(data.outcome);
                    addHistory(data.outcome);
                    updateStats(data);
                    busy = false;
                });
            })
            .catch(function () {
                fail("error");
                button.disabled = false;
                busy = false;
            });
    });

    if (window.I18N) window.I18N.onChange(idle);
    idle();
    window.addEventListener("resize", function () {
        if (!busy && !reel.classList.contains("has-result")) idle();
    });
})();
