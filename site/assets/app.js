// The documentation website's behaviour: the theme, the pages' drawer on
// narrow windows, the search, copying code, and the outline following
// the reading.
(function () {
  'use strict';

  var root = document.body.getAttribute('data-root') || '';

  // ----- the theme: the system's, light or dark, in turn

  var theme = document.querySelector('.theme');
  if (theme) {
    var themes = ['system', 'light', 'dark'];
    var names = { system: 'the system\'s', light: 'light', dark: 'dark' };
    var picked = function () {
      return document.documentElement.dataset.theme || 'system';
    };
    var after = function (name) {
      return themes[(themes.indexOf(name) + 1) % themes.length];
    };
    var describe = function () {
      var text = 'Theme: ' + names[picked()] + '; switch to ' + names[after(picked())];
      theme.setAttribute('aria-label', text);
      theme.title = text;
    };
    describe();
    theme.addEventListener('click', function () {
      var next = after(picked());
      try {
        if (next === 'system') localStorage.removeItem('theme');
        else localStorage.setItem('theme', next);
      } catch (e) {}
      if (next === 'system') delete document.documentElement.dataset.theme;
      else document.documentElement.dataset.theme = next;
      describe();
    });
  }

  // ----- the pages, in a drawer on narrow windows

  var menu = document.querySelector('.menu');
  function setDrawer(open) {
    document.body.classList.toggle('nav-open', open);
    if (menu) menu.setAttribute('aria-expanded', String(open));
  }
  if (menu) {
    menu.addEventListener('click', function () {
      setDrawer(!document.body.classList.contains('nav-open'));
    });
  }
  document.addEventListener('click', function (event) {
    if (!document.body.classList.contains('nav-open')) return;
    if (event.target.closest('.sidebar a') || (!event.target.closest('.sidebar') && !event.target.closest('.menu'))) {
      setDrawer(false);
    }
  });
  var current = document.querySelector('.sidebar a.current');
  if (current && current.scrollIntoView) current.scrollIntoView({ block: 'center' });

  // ----- copying code

  document.querySelectorAll('.doc .code').forEach(function (block) {
    var language = block.getAttribute('data-lang');
    if (language) {
      var label = document.createElement('span');
      label.className = 'lang';
      label.textContent = language;
      block.appendChild(label);
    }
    var button = document.createElement('button');
    button.type = 'button';
    button.className = 'copy';
    button.setAttribute('aria-label', 'Copy the code');
    button.innerHTML =
      '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>';
    button.addEventListener('click', function () {
      var text = block.querySelector('pre').innerText;
      var done = function () {
        button.classList.add('copied');
        button.setAttribute('aria-label', 'Copied');
        setTimeout(function () {
          button.classList.remove('copied');
          button.setAttribute('aria-label', 'Copy the code');
        }, 1500);
      };
      if (navigator.clipboard) {
        navigator.clipboard.writeText(text).then(done, function () {});
      }
    });
    block.appendChild(button);
  });

  // ----- the outline: the heading being read

  var links = Array.prototype.slice.call(document.querySelectorAll('.outline a'));
  if (links.length && 'IntersectionObserver' in window) {
    var targets = links
      .map(function (link) {
        return document.getElementById(decodeURIComponent(link.hash.slice(1)));
      })
      .filter(Boolean);
    var mark = function () {
      var line = 96;
      var active = targets[0];
      for (var i = 0; i < targets.length; i++) {
        if (targets[i].getBoundingClientRect().top - line <= 0) active = targets[i];
      }
      if (window.innerHeight + window.scrollY >= document.body.scrollHeight - 2) {
        active = targets[targets.length - 1];
      }
      links.forEach(function (link) {
        link.classList.toggle('active', active && link.hash === '#' + active.id);
      });
    };
    var pending = false;
    window.addEventListener(
      'scroll',
      function () {
        if (pending) return;
        pending = true;
        requestAnimationFrame(function () {
          pending = false;
          mark();
        });
      },
      { passive: true }
    );
    mark();
  }

  // ----- the search

  var dialog = document.querySelector('.search');
  var input = dialog && dialog.querySelector('input');
  var list = dialog && dialog.querySelector('.results');
  var opener = document.querySelector('.search-open');
  var selected = 0;
  var lastFocus = null;

  function escapeHtml(text) {
    return text.replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  }

  function highlight(text, terms) {
    var html = escapeHtml(text);
    terms.forEach(function (term) {
      var pattern = new RegExp('(' + escapeHtml(term).replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
      html = html.replace(pattern, '<mark>$1</mark>');
    });
    return html;
  }

  function snippet(text, terms) {
    var lower = text.toLowerCase();
    var at = -1;
    terms.forEach(function (term) {
      var found = lower.indexOf(term);
      if (found >= 0 && (at < 0 || found < at)) at = found;
    });
    if (at < 0) return text.slice(0, 160);
    var start = Math.max(0, at - 60);
    return (start > 0 ? '… ' : '') + text.slice(start, start + 200);
  }

  function search(query) {
    var terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!terms.length) return [];
    var found = [];
    (window.ORG_SITE_SEARCH || []).forEach(function (entry) {
      var heading = entry.heading.toLowerCase();
      var text = entry.text.toLowerCase();
      var page = entry.page.toLowerCase();
      var score = 0;
      for (var i = 0; i < terms.length; i++) {
        var term = terms[i];
        var inHeading = heading.indexOf(term) >= 0;
        var inText = text.indexOf(term) >= 0;
        var inPage = page.indexOf(term) >= 0;
        if (!inHeading && !inText && !inPage) return;
        score += (inHeading ? 10 : 0) + (inPage ? 3 : 0) + (inText ? 1 : 0);
        if (heading === term) score += 10;
      }
      found.push({ entry: entry, score: score });
    });
    found.sort(function (a, b) {
      return b.score - a.score;
    });
    return found.slice(0, 40).map(function (item) {
      return item.entry;
    });
  }

  function render() {
    var query = input.value.trim();
    var terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    var results = search(query);
    selected = 0;
    if (!query) {
      list.innerHTML = '';
      return;
    }
    if (!results.length) {
      list.innerHTML = '<li class="none">Nothing found for “' + escapeHtml(query) + '”</li>';
      return;
    }
    list.innerHTML = results
      .map(function (entry, index) {
        return (
          '<li role="option" id="result-' + index + '" aria-selected="' + (index === 0) + '">' +
          '<a href="' + root + entry.url + '">' +
          '<span class="where">' + escapeHtml(entry.page) + '</span>' +
          '<span class="heading">' + highlight(entry.heading, terms) + '</span>' +
          '<span class="snippet">' + highlight(snippet(entry.text, terms), terms) + '</span>' +
          '</a></li>'
        );
      })
      .join('');
  }

  function select(index) {
    var items = list.querySelectorAll('[role="option"]');
    if (!items.length) return;
    selected = (index + items.length) % items.length;
    items.forEach(function (item, i) {
      item.setAttribute('aria-selected', String(i === selected));
    });
    items[selected].scrollIntoView({ block: 'nearest' });
    input.setAttribute('aria-activedescendant', items[selected].id);
  }

  function open() {
    if (!dialog) return;
    lastFocus = document.activeElement;
    dialog.hidden = false;
    input.focus();
    input.select();
    render();
  }

  function close() {
    dialog.hidden = true;
    if (lastFocus && lastFocus.focus) lastFocus.focus();
  }

  if (dialog) {
    opener.addEventListener('click', open);
    input.addEventListener('input', render);
    input.addEventListener('keydown', function (event) {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        select(selected + 1);
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        select(selected - 1);
      } else if (event.key === 'Enter') {
        var link = list.querySelectorAll('[role="option"] a')[selected];
        if (link) {
          event.preventDefault();
          close();
          window.location.href = link.href;
        }
      }
    });
    dialog.addEventListener('click', function (event) {
      if (event.target === dialog) close();
      if (event.target.closest('.results a')) close();
    });
    document.addEventListener('keydown', function (event) {
      var typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName) || document.activeElement.isContentEditable;
      if (event.key === 'Escape') {
        if (!dialog.hidden) close();
        else setDrawer(false);
      } else if (dialog.hidden && !typing && (event.key === '/' || ((event.ctrlKey || event.metaKey) && event.key === 'k'))) {
        event.preventDefault();
        open();
      }
    });
  }
})();
