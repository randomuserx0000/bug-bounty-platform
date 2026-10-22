// Interacciones del sitio público (home y listado de programas).
// Sin dependencias. Cada bloque se activa solo si su marcado está en la página.
(function () {
    'use strict';

    var reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    function onReady(fn) {
        if (document.readyState !== 'loading') fn();
        else document.addEventListener('DOMContentLoaded', fn);
    }

    // ---------- Home: selector "Soy empresa / Soy investigador" ----------
    function initAudience() {
        var buttons = document.querySelectorAll('[data-aud]');
        if (!buttons.length) return;
        buttons.forEach(function (btn) {
            btn.addEventListener('click', function () {
                var aud = btn.getAttribute('data-aud');
                buttons.forEach(function (b) { b.setAttribute('aria-pressed', b === btn ? 'true' : 'false'); });
                document.querySelectorAll('[data-aud-panel]').forEach(function (p) {
                    p.hidden = p.getAttribute('data-aud-panel') !== aud;
                });
            });
        });
    }

    // ---------- Home: pausar/reanudar la consola ----------
    function initConsole() {
        var box = document.querySelector('[data-console]');
        if (!box) return;
        var btn = box.querySelector('[data-console-toggle]');
        var label = box.querySelector('[data-console-label]');
        btn.addEventListener('click', function () {
            var paused = box.classList.toggle('paused');
            btn.setAttribute('aria-pressed', paused ? 'true' : 'false');
            label.textContent = paused ? 'Reanudar' : 'Pausar';
            var steps = document.querySelector('[data-steps]');
            if (steps) steps.classList.toggle('paused', paused);
        });
    }

    // ---------- Home: cifras que cuentan hacia arriba ----------
    function initCountUp() {
        var els = document.querySelectorAll('[data-count]');
        if (!els.length || reduced) return;
        var fmt = function (n) { return Math.round(n).toLocaleString('es-VE'); };
        var targets = [];
        els.forEach(function (el) {
            targets.push({ el: el, to: parseFloat(el.getAttribute('data-count')) || 0 });
            el.textContent = '0';
        });
        var t0 = performance.now();
        function tick(t) {
            var k = Math.min(1, (t - t0) / 1600);
            var e = 1 - Math.pow(1 - k, 3);
            targets.forEach(function (x) { x.el.textContent = fmt(x.to * e); });
            if (k < 1) requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
    }

    // ---------- Home: pestañas de pasos con avance automático ----------
    function initSteps() {
        var root = document.querySelector('[data-steps]');
        if (!root) return;
        var tabs = Array.prototype.slice.call(root.querySelectorAll('[role="tab"]'));
        var current = 0;
        var timer = null;

        function select(i, focus) {
            current = (i + tabs.length) % tabs.length;
            tabs.forEach(function (tab, j) {
                var on = j === current;
                tab.setAttribute('aria-selected', on ? 'true' : 'false');
                tab.tabIndex = on ? 0 : -1;
                var panel = document.getElementById(tab.getAttribute('aria-controls'));
                if (panel) panel.hidden = !on;
            });
            if (focus) tabs[current].focus();
            // Reinicia la barra de progreso de la pestaña activa.
            var bar = tabs[current].querySelector('.step-bar > span');
            if (bar) { bar.style.animation = 'none'; void bar.offsetWidth; bar.style.animation = ''; }
        }
        function restart() {
            clearInterval(timer);
            if (reduced) { root.classList.remove('autoplay'); return; }
            timer = setInterval(function () {
                if (root.classList.contains('paused')) return;
                select(current + 1, false);
            }, 6000);
        }
        tabs.forEach(function (tab, i) {
            tab.addEventListener('click', function () { select(i, false); restart(); });
            tab.addEventListener('keydown', function (e) {
                if (e.key === 'ArrowDown' || e.key === 'ArrowRight') { e.preventDefault(); select(current + 1, true); restart(); }
                if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') { e.preventDefault(); select(current - 1, true); restart(); }
            });
        });
        select(0, false);
        restart();
    }

    // ---------- Listado: buscar, filtrar y ordenar programas ----------
    function initExplore() {
        var root = document.querySelector('[data-explore]');
        if (!root) return;
        var list = root.querySelector('[data-rows]');
        var rows = Array.prototype.slice.call(list.querySelectorAll('.prog-row'));
        var q = root.querySelector('[data-filter-q]');
        var typeBtns = root.querySelectorAll('[data-filter-type] [data-type]');
        var rangeBoxes = root.querySelectorAll('[data-filter-range] input');
        var sortBtns = root.querySelectorAll('[data-sort] [data-sort-key]');
        var countLabel = root.querySelector('[data-count-label]');
        var empty = root.querySelector('[data-empty]');
        var state = { type: '', sort: 'reward' };

        // Recompensa más alta en la cabecera.
        var top = document.querySelector('[data-top-reward]');
        if (top) {
            var max = rows.reduce(function (m, r) { return Math.max(m, parseInt(r.dataset.max, 10) || 0); }, 0);
            top.textContent = '';
            if (max > 0) {
                var cur = document.createElement('span');
                cur.className = 'cur';
                cur.textContent = '$';
                top.appendChild(cur);
                top.appendChild(document.createTextNode(Math.floor(max / 100).toLocaleString('es-VE')));
            } else {
                top.textContent = '—';
            }
        }

        var ranges = {
            hi: function (usd) { return usd > 2500; },
            mid: function (usd) { return usd >= 1000 && usd <= 2500; },
            lo: function (usd) { return usd < 1000; }
        };

        function apply() {
            var term = (q.value || '').trim().toLowerCase();
            var active = Array.prototype.filter.call(rangeBoxes, function (b) { return b.checked; })
                .map(function (b) { return ranges[b.value]; });
            var shown = 0;
            rows.forEach(function (r) {
                var usd = (parseInt(r.dataset.max, 10) || 0) / 100;
                var ok = true;
                if (state.type && (' ' + r.dataset.assets + ' ').indexOf(' ' + state.type + ' ') === -1) ok = false;
                if (ok && term && r.dataset.name.toLowerCase().indexOf(term) === -1) ok = false;
                if (ok && active.length && !active.some(function (f) { return f(usd); })) ok = false;
                r.hidden = !ok;
                if (ok) shown++;
            });
            rows.slice().sort(function (a, b) {
                return state.sort === 'reward'
                    ? (parseInt(b.dataset.max, 10) || 0) - (parseInt(a.dataset.max, 10) || 0)
                    : (parseInt(b.dataset.updated, 10) || 0) - (parseInt(a.dataset.updated, 10) || 0);
            }).forEach(function (r) { list.appendChild(r); });
            countLabel.textContent = shown === 1 ? '1 programa' : shown + ' programas';
            empty.hidden = shown !== 0;
        }

        q.addEventListener('input', apply);
        typeBtns.forEach(function (btn) {
            btn.addEventListener('click', function () {
                state.type = btn.getAttribute('data-type');
                typeBtns.forEach(function (b) { b.setAttribute('aria-pressed', b === btn ? 'true' : 'false'); });
                apply();
            });
        });
        rangeBoxes.forEach(function (b) { b.addEventListener('change', apply); });
        sortBtns.forEach(function (btn) {
            btn.addEventListener('click', function () {
                state.sort = btn.getAttribute('data-sort-key');
                sortBtns.forEach(function (b) { b.setAttribute('aria-pressed', b === btn ? 'true' : 'false'); });
                apply();
            });
        });
        root.querySelector('[data-reset]').addEventListener('click', function () {
            q.value = '';
            rangeBoxes.forEach(function (b) { b.checked = false; });
            typeBtns[0].click();
        });
        apply();
    }

    onReady(function () {
        initAudience();
        initConsole();
        initCountUp();
        initSteps();
        initExplore();
    });
})();
