#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use url::Url;
use wry::PageLoadEvent;

const KEEP_URL: &str = "https://keep.google.com";
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 3.0;
const ZOOM_STEP: f64 = 0.1;

static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);
static NEXT_SUBMIT_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "windows")]
const AUTOSTART_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const AUTOSTART_VALUE_NAME: &str = "GoogleKeepDesktop";
#[cfg(target_os = "windows")]
const AUTOSTART_MENU_LABEL: &str = "Iniciar com o Windows";
#[cfg(not(target_os = "windows"))]
const AUTOSTART_MENU_LABEL: &str = "Iniciar com o sistema";

const QUICK_NOTE_HTML: &str = r#"<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Nova nota rápida</title>
<style>
  :root {
    color-scheme: dark;
    --bg: #202124;
    --surface: #252629;
    --border: #3c4043;
    --subtle-border: rgba(255, 255, 255, 0.08);
  }
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }
  html, body {
    width: 100%;
    height: 100%;
    background: var(--bg);
    color: #e8eaed;
    font-family: 'Segoe UI', system-ui, -apple-system, Roboto, sans-serif;
    overflow: hidden;
    user-select: none;
  }
  .card {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    transition: background-color 0.18s ease, border-color 0.18s ease;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 12px;
    border-bottom: 1px solid var(--subtle-border);
    background: rgba(0, 0, 0, 0.16);
    cursor: grab;
  }
  .header:active {
    cursor: grabbing;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: #e8eaed;
  }
  .brand-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #fbbc04;
    box-shadow: 0 0 8px rgba(251, 188, 4, 0.5);
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .icon-btn {
    width: 26px;
    height: 26px;
    border: 1px solid transparent;
    background: transparent;
    color: #9aa0a6;
    border-radius: 6px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #e8eaed;
  }
  .icon-btn.active {
    background: rgba(251, 188, 4, 0.18);
    color: #fbbc04;
    border-color: rgba(251, 188, 4, 0.45);
  }
  .link-btn {
    border: 1px solid rgba(255, 255, 255, 0.14);
    background: transparent;
    color: #bdc1c6;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 5px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .link-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
  }
  .fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 10px 14px 6px;
    gap: 6px;
    min-height: 0;
    overflow-y: auto;
  }
  .fields::-webkit-scrollbar {
    width: 6px;
  }
  .fields::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.18);
    border-radius: 999px;
  }
  #note-title {
    width: 100%;
    border: none;
    outline: none;
    background: transparent;
    color: #e8eaed;
    font-size: 16px;
    font-weight: 600;
    font-family: inherit;
    padding: 3px 0;
    user-select: text;
  }
  #note-title::placeholder {
    color: rgba(232, 234, 237, 0.45);
  }
  #note-body {
    flex: 1;
    width: 100%;
    min-height: 92px;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    color: #e8eaed;
    font-size: 14px;
    line-height: 1.5;
    font-family: inherit;
    padding: 2px 0;
    user-select: text;
  }
  #note-body::placeholder {
    color: rgba(232, 234, 237, 0.45);
  }
  .checklist-wrap {
    display: none;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .checklist-wrap.visible {
    display: flex;
  }
  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .check-row input[type="checkbox"] {
    width: 15px;
    height: 15px;
    accent-color: #fbbc04;
    cursor: pointer;
    flex-shrink: 0;
  }
  .check-row input[type="text"] {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    color: #e8eaed;
    font-size: 14px;
    font-family: inherit;
    user-select: text;
  }
  .check-row input[type="text"].checked {
    text-decoration: line-through;
    opacity: 0.6;
  }
  .check-del {
    border: none;
    background: transparent;
    color: rgba(232, 234, 237, 0.45);
    cursor: pointer;
    font-size: 15px;
    padding: 0 4px;
    border-radius: 4px;
  }
  .check-del:hover {
    color: #e8eaed;
    background: rgba(255, 255, 255, 0.08);
  }
  .add-item-btn {
    align-self: flex-start;
    margin-top: 2px;
    border: none;
    background: transparent;
    color: rgba(232, 234, 237, 0.65);
    font-size: 12px;
    cursor: pointer;
    padding: 3px 4px;
    border-radius: 4px;
  }
  .add-item-btn:hover {
    color: #fbbc04;
    background: rgba(255, 255, 255, 0.06);
  }
  .tags-bar {
    display: none;
    flex-direction: column;
    gap: 6px;
    padding: 7px 14px 8px;
    border-top: 1px solid var(--subtle-border);
    background: rgba(0, 0, 0, 0.14);
  }
  .tags-bar.visible {
    display: flex;
  }
  .tags-selected {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }
  .tags-selected:empty {
    display: none;
  }
  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(251, 188, 4, 0.18);
    border: 1px solid rgba(251, 188, 4, 0.45);
    color: #fdd663;
    font-size: 11px;
    font-weight: 500;
    padding: 2px 7px;
    border-radius: 999px;
  }
  .tag-chip button {
    border: none;
    background: transparent;
    color: #fdd663;
    cursor: pointer;
    font-size: 12px;
    line-height: 1;
    padding: 0 1px;
  }
  .tags-input-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tag-refresh-btn {
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: #bdc1c6;
    font-size: 13px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }
  .tag-refresh-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  #tag-input {
    flex: 1;
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 6px;
    outline: none;
    background: rgba(255, 255, 255, 0.05);
    color: #e8eaed;
    font-size: 12px;
    font-family: inherit;
    padding: 4px 8px;
    user-select: text;
  }
  #tag-input:focus {
    border-color: rgba(251, 188, 4, 0.6);
  }
  #tag-input::placeholder {
    color: rgba(232, 234, 237, 0.45);
  }
  .labels-list {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    max-height: 92px;
    overflow-y: auto;
    padding-right: 2px;
  }
  .labels-list::-webkit-scrollbar {
    width: 5px;
  }
  .labels-list::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.18);
    border-radius: 999px;
  }
  .label-opt {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.05);
    color: #bdc1c6;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 999px;
    cursor: pointer;
    transition: background 0.14s, color 0.14s, border-color 0.14s;
  }
  .label-opt:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .label-opt.selected {
    background: rgba(251, 188, 4, 0.2);
    border-color: rgba(251, 188, 4, 0.6);
    color: #fbbc04;
    font-weight: 600;
  }
  .label-opt.create-opt {
    border-style: dashed;
    border-color: rgba(129, 201, 149, 0.55);
    color: #81c995;
    background: rgba(129, 201, 149, 0.1);
  }
  .label-opt.create-opt:hover {
    background: rgba(129, 201, 149, 0.2);
    color: #a8dab5;
  }
  .labels-empty {
    font-size: 11px;
    color: rgba(232, 234, 237, 0.45);
    padding: 2px 0;
  }
  .palette-bar {
    display: none;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-top: 1px solid var(--subtle-border);
    background: rgba(0, 0, 0, 0.12);
    overflow-x: auto;
  }
  .palette-bar.visible {
    display: flex;
  }
  .swatch {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.22);
    cursor: pointer;
    flex-shrink: 0;
    transition: transform 0.12s, border-color 0.12s;
  }
  .swatch:hover {
    transform: scale(1.14);
  }
  .swatch.active {
    border-color: #fbbc04;
    transform: scale(1.12);
  }
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-top: 1px solid var(--subtle-border);
    background: rgba(0, 0, 0, 0.18);
    gap: 8px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .hint {
    font-size: 11px;
    color: rgba(232, 234, 237, 0.65);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint.warn {
    color: #fbbc04;
  }
  .hint.ok {
    color: #81c995;
    font-weight: 600;
  }
  .draft-btn {
    display: none;
    border: 1px solid rgba(251, 188, 4, 0.45);
    background: rgba(251, 188, 4, 0.12);
    color: #fbbc04;
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 5px;
    cursor: pointer;
  }
  .draft-btn.visible {
    display: inline-block;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn-ghost {
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: transparent;
    color: #e8eaed;
    font-size: 12px;
    font-weight: 500;
    padding: 5px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn-ghost:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .btn-primary {
    border: none;
    background: #fbbc04;
    color: #202124;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn-primary:hover {
    background: #fcc934;
  }
</style>
</head>
<body>
  <div class="card" id="card">
    <div class="header" id="drag-header" title="Arraste para mover · Clique duplo para centralizar">
      <div class="brand">
        <span class="brand-dot"></span>
        <span>Nova nota rápida</span>
      </div>
      <div class="header-actions">
        <button id="btn-pin" class="icon-btn" type="button" title="Fixar nota no topo">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16h14v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>
          </svg>
        </button>
        <button id="btn-open-main" class="link-btn" type="button" title="Abrir janela completa do Keep">Abrir Keep</button>
        <button id="btn-close" class="icon-btn" type="button" title="Fechar (Esc)" style="font-size:18px">&times;</button>
      </div>
    </div>

    <div class="fields" id="fields-wrap">
      <input id="note-title" type="text" placeholder="Título" autocomplete="off" />
      <textarea id="note-body" placeholder="Criar uma nota..."></textarea>
      <div id="checklist-wrap" class="checklist-wrap"></div>
    </div>

    <div id="tags-bar" class="tags-bar">
      <div id="tags-chips" class="tags-selected"></div>
      <div class="tags-input-row">
        <input id="tag-input" type="text" placeholder="Buscar ou criar marcador (Enter)..." autocomplete="off" />
        <button id="btn-refresh-labels" class="tag-refresh-btn" type="button" title="Sincronizar marcadores do Keep">&#x21bb;</button>
      </div>
      <div id="labels-list" class="labels-list"></div>
    </div>

    <div id="palette-bar" class="palette-bar"></div>

    <div class="footer">
      <div class="tools">
        <button id="btn-checklist" class="icon-btn" type="button" title="Alternar modo checklist">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="5" width="6" height="6" rx="1"/><path d="m4.5 8 1.2 1.2L8 6.8"/><line x1="13" y1="8" x2="21" y2="8"/>
            <rect x="3" y="14" width="6" height="6" rx="1"/><line x1="13" y1="17" x2="21" y2="17"/>
          </svg>
        </button>
        <button id="btn-palette" class="icon-btn" type="button" title="Cor da nota">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/>
            <circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/>
            <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/>
          </svg>
        </button>
        <button id="btn-tags" class="icon-btn" type="button" title="Marcadores">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 2H2v10l9.29 9.29c.94.94 2.48.94 3.42 0l6.58-6.58c.94-.94.94-2.48 0-3.42L12 2Z"/>
            <circle cx="7" cy="7" r="1"/>
          </svg>
        </button>
        <button id="btn-restore-draft" class="draft-btn" type="button" title="Restaurar último rascunho">Rascunho</button>
      </div>
      <span id="hint-text" class="hint">Ctrl+Enter salvar</span>
      <div class="actions">
        <button id="btn-cancel" class="btn-ghost" type="button">Cancelar</button>
        <button id="btn-save" class="btn-primary" type="button">Salvar</button>
      </div>
    </div>
  </div>

  <script>
    (function () {
      var COLORS = [
        { id: 'default', bg: '#202124', name: 'Padrão', labels: [] },
        { id: 'coral', bg: '#77172e', name: 'Coral', labels: ['Coral', 'Vermelho', 'Red'] },
        { id: 'peach', bg: '#692b17', name: 'Pêssego', labels: ['Pêssego', 'Peach', 'Laranja', 'Orange'] },
        { id: 'sand', bg: '#7c4a03', name: 'Areia', labels: ['Areia', 'Sand', 'Amarelo', 'Yellow'] },
        { id: 'mint', bg: '#264d3b', name: 'Menta', labels: ['Menta', 'Mint', 'Verde', 'Green'] },
        { id: 'sage', bg: '#0c625d', name: 'Sálvia', labels: ['Sálvia', 'Sage', 'Azul-petróleo', 'Teal'] },
        { id: 'fog', bg: '#256377', name: 'Névoa', labels: ['Névoa', 'Fog', 'Azul', 'Blue'] },
        { id: 'storm', bg: '#284255', name: 'Crepúsculo', labels: ['Crepúsculo', 'Storm', 'Azul-escuro', 'Dark blue'] },
        { id: 'blossom', bg: '#472e5b', name: 'Flor', labels: ['Flor', 'Blossom', 'Roxo', 'Purple'] },
        { id: 'clay', bg: '#6c394f', name: 'Argila', labels: ['Argila', 'Clay', 'Rosa', 'Pink'] },
        { id: 'chalk', bg: '#4b443a', name: 'Giz', labels: ['Giz', 'Chalk', 'Marrom', 'Brown', 'Cinza', 'Gray'] }
      ];

      var DRAFT_KEY = 'kd_quick_note_discarded_v1';
      var DEFAULT_HINT = 'Ctrl+Enter salvar';

      var titleEl = document.getElementById('note-title');
      var bodyEl = document.getElementById('note-body');
      var checklistWrap = document.getElementById('checklist-wrap');
      var paletteBar = document.getElementById('palette-bar');
      var tagsBar = document.getElementById('tags-bar');
      var tagsChips = document.getElementById('tags-chips');
      var tagInput = document.getElementById('tag-input');
      var labelsList = document.getElementById('labels-list');
      var hintEl = document.getElementById('hint-text');
      var btnPin = document.getElementById('btn-pin');
      var btnChecklist = document.getElementById('btn-checklist');
      var btnPalette = document.getElementById('btn-palette');
      var btnTags = document.getElementById('btn-tags');
      var btnRefreshLabels = document.getElementById('btn-refresh-labels');
      var btnRestore = document.getElementById('btn-restore-draft');

      var state = {
        colorId: 'default',
        pinned: false,
        isChecklist: false,
        items: [{ text: '', checked: false }],
        tags: [],
        availableLabels: [],
        paletteOpen: false,
        tagsOpen: false,
        saving: false
      };

      function clearChildren(el) {
        while (el.firstChild) {
          el.removeChild(el.firstChild);
        }
      }

      function send(payload) {
        if (window.ipc && typeof window.ipc.postMessage === 'function') {
          window.ipc.postMessage(JSON.stringify(payload));
        }
      }

      function applyColor(colorId) {
        var found = COLORS.find(function (c) { return c.id === colorId; }) || COLORS[0];
        state.colorId = found.id;
        document.documentElement.style.setProperty('--bg', found.bg);
        var swatches = paletteBar.querySelectorAll('.swatch');
        swatches.forEach(function (sw) {
          sw.classList.toggle('active', sw.dataset.id === found.id);
        });
      }

      COLORS.forEach(function (c) {
        var sw = document.createElement('button');
        sw.type = 'button';
        sw.className = 'swatch' + (c.id === 'default' ? ' active' : '');
        sw.style.backgroundColor = c.bg;
        sw.title = c.name;
        sw.dataset.id = c.id;
        sw.addEventListener('click', function () {
          applyColor(c.id);
        });
        paletteBar.appendChild(sw);
      });

      function requestResize() {
        var extra = 0;
        if (state.paletteOpen) extra += 34;
        if (state.tagsOpen) extra += 136;
        if (state.isChecklist) {
          extra += Math.min(Math.max(0, state.items.length - 3) * 28, 120);
        } else {
          var lines = (bodyEl.value.match(/\n/g) || []).length;
          extra += Math.min(Math.max(0, lines - 3) * 20, 110);
        }
        var targetH = Math.min(540, Math.max(330, 330 + extra));
        send({ action: 'resize', height: targetH });
      }

      function findSelectedIndex(name) {
        var target = name.toLowerCase();
        for (var i = 0; i < state.tags.length; i++) {
          if (state.tags[i].toLowerCase() === target) return i;
        }
        return -1;
      }

      function toggleTag(name) {
        var clean = String(name || '').replace(/^#+/, '').trim();
        if (!clean) return;
        var idx = findSelectedIndex(clean);
        if (idx !== -1) {
          state.tags.splice(idx, 1);
        } else {
          state.tags.push(clean);
        }
        var existsInAvail = state.availableLabels.some(function (l) {
          return l.toLowerCase() === clean.toLowerCase();
        });
        if (!existsInAvail) {
          state.availableLabels.push(clean);
          state.availableLabels.sort(function (a, b) {
            return a.localeCompare(b, undefined, { sensitivity: 'base' });
          });
        }
        renderTags();
        if (state.tagsOpen) requestResize();
      }

      function renderTags() {
        clearChildren(tagsChips);
        clearChildren(labelsList);

        btnTags.classList.toggle('active', state.tagsOpen || state.tags.length > 0);

        state.tags.forEach(function (tag, idx) {
          var chip = document.createElement('span');
          chip.className = 'tag-chip';
          chip.appendChild(document.createTextNode('#' + tag + ' '));
          var rm = document.createElement('button');
          rm.type = 'button';
          rm.textContent = '\u00d7';
          rm.addEventListener('click', function () {
            state.tags.splice(idx, 1);
            renderTags();
          });
          chip.appendChild(rm);
          tagsChips.appendChild(chip);
        });

        var queryRaw = tagInput.value.replace(/^#+/, '').trim();
        var query = queryRaw.toLowerCase();

        var poolMap = Object.create(null);
        var allLabels = [];
        state.availableLabels.concat(state.tags).forEach(function (lbl) {
          var clean = String(lbl || '').trim();
          if (!clean) return;
          var key = clean.toLowerCase();
          if (!poolMap[key]) {
            poolMap[key] = clean;
            allLabels.push(clean);
          }
        });
        allLabels.sort(function (a, b) {
          return a.localeCompare(b, undefined, { sensitivity: 'base' });
        });

        var exactExists = query ? !!poolMap[query] : false;
        var filtered = allLabels.filter(function (lbl) {
          return !query || lbl.toLowerCase().indexOf(query) !== -1;
        });

        filtered.forEach(function (lbl) {
          var isSel = findSelectedIndex(lbl) !== -1;
          var btn = document.createElement('button');
          btn.type = 'button';
          btn.className = 'label-opt' + (isSel ? ' selected' : '');
          btn.textContent = (isSel ? '\u2713 ' : '') + lbl;
          btn.addEventListener('click', function () {
            toggleTag(lbl);
            tagInput.value = '';
            renderTags();
            tagInput.focus();
          });
          labelsList.appendChild(btn);
        });

        if (queryRaw && !exactExists) {
          var createBtn = document.createElement('button');
          createBtn.type = 'button';
          createBtn.className = 'label-opt create-opt';
          createBtn.textContent = '+ Criar marcador "' + queryRaw + '"';
          createBtn.addEventListener('click', function () {
            toggleTag(queryRaw);
            tagInput.value = '';
            renderTags();
            tagInput.focus();
          });
          labelsList.appendChild(createBtn);
        } else if (!filtered.length) {
          var empty = document.createElement('span');
          empty.className = 'labels-empty';
          empty.textContent = 'Nenhum marcador encontrado \u2014 digite acima para criar';
          labelsList.appendChild(empty);
        }
      }

      function addTagFromInput() {
        var raw = tagInput.value.replace(/^#+/, '').trim();
        if (!raw) return;
        raw.split(',').forEach(function (part) {
          var clean = part.replace(/^#+/, '').trim();
          if (!clean) return;
          var existing = state.availableLabels.find(function (l) {
            return l.toLowerCase() === clean.toLowerCase();
          });
          var target = existing || clean;
          if (findSelectedIndex(target) === -1) {
            state.tags.push(target);
          }
          if (!existing) {
            state.availableLabels.push(clean);
          }
        });
        state.availableLabels.sort(function (a, b) {
          return a.localeCompare(b, undefined, { sensitivity: 'base' });
        });
        tagInput.value = '';
        renderTags();
      }

      window.setAvailableLabels = function (labels) {
        if (!Array.isArray(labels)) return;
        var map = Object.create(null);
        var next = [];
        labels.forEach(function (l) {
          var clean = String(l || '').trim();
          if (!clean) return;
          var key = clean.toLowerCase();
          if (!map[key]) {
            map[key] = 1;
            next.push(clean);
          }
        });
        next.sort(function (a, b) {
          return a.localeCompare(b, undefined, { sensitivity: 'base' });
        });
        state.availableLabels = next;
        renderTags();
      };

      function renderChecklist(focusIndex) {
        clearChildren(checklistWrap);
        if (!state.items.length) {
          state.items.push({ text: '', checked: false });
        }
        state.items.forEach(function (item, idx) {
          var row = document.createElement('div');
          row.className = 'check-row';

          var cb = document.createElement('input');
          cb.type = 'checkbox';
          cb.checked = !!item.checked;
          cb.addEventListener('change', function () {
            item.checked = cb.checked;
            txt.classList.toggle('checked', item.checked);
          });

          var txt = document.createElement('input');
          txt.type = 'text';
          txt.placeholder = 'Item da lista...';
          txt.value = item.text || '';
          if (item.checked) txt.classList.add('checked');
          txt.addEventListener('input', function () {
            item.text = txt.value;
          });
          txt.addEventListener('keydown', function (e) {
            if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
              e.preventDefault();
              state.items.splice(idx + 1, 0, { text: '', checked: false });
              renderChecklist(idx + 1);
              requestResize();
            } else if (e.key === 'Backspace' && !txt.value && state.items.length > 1) {
              e.preventDefault();
              state.items.splice(idx, 1);
              renderChecklist(Math.max(0, idx - 1));
              requestResize();
            }
          });

          var del = document.createElement('button');
          del.type = 'button';
          del.className = 'check-del';
          del.textContent = '\u00d7';
          del.addEventListener('click', function () {
            if (state.items.length > 1) {
              state.items.splice(idx, 1);
            } else {
              state.items[0] = { text: '', checked: false };
            }
            renderChecklist(Math.max(0, idx - 1));
            requestResize();
          });

          row.appendChild(cb);
          row.appendChild(txt);
          row.appendChild(del);
          checklistWrap.appendChild(row);
        });

        var addBtn = document.createElement('button');
        addBtn.type = 'button';
        addBtn.className = 'add-item-btn';
        addBtn.textContent = '+ Novo item';
        addBtn.addEventListener('click', function () {
          state.items.push({ text: '', checked: false });
          renderChecklist(state.items.length - 1);
          requestResize();
        });
        checklistWrap.appendChild(addBtn);

        if (typeof focusIndex === 'number') {
          var inputs = checklistWrap.querySelectorAll('input[type="text"]');
          if (inputs[focusIndex]) {
            setTimeout(function () { inputs[focusIndex].focus(); }, 10);
          }
        }
      }

      function setChecklistMode(enabled, skipConvert) {
        state.isChecklist = enabled;
        btnChecklist.classList.toggle('active', enabled);
        if (enabled) {
          if (!skipConvert) {
            var lines = bodyEl.value.split(/\r?\n/).map(function (l) {
              return l.replace(/^\s*[-*]\s*(\[[ xX]\]\s*)?/, '').trim();
            }).filter(Boolean);
            state.items = lines.length
              ? lines.map(function (t) { return { text: t, checked: false }; })
              : [{ text: '', checked: false }];
          }
          bodyEl.style.display = 'none';
          checklistWrap.classList.add('visible');
          renderChecklist(state.items.length - 1);
        } else {
          if (!skipConvert) {
            bodyEl.value = state.items.map(function (i) { return i.text; }).filter(Boolean).join('\n');
          }
          checklistWrap.classList.remove('visible');
          bodyEl.style.display = '';
          setTimeout(function () { bodyEl.focus(); }, 10);
        }
        requestResize();
      }

      function hasContent() {
        if (titleEl.value.trim()) return true;
        if (!state.isChecklist && bodyEl.value.trim()) return true;
        if (state.isChecklist && state.items.some(function (i) { return (i.text || '').trim(); })) return true;
        return false;
      }

      function saveDiscardedDraftIfAny() {
        try {
          if (hasContent()) {
            localStorage.setItem(DRAFT_KEY, JSON.stringify({
              title: titleEl.value,
              body: bodyEl.value,
              isChecklist: state.isChecklist,
              items: state.items,
              colorId: state.colorId,
              pinned: state.pinned,
              tags: state.tags
            }));
          }
        } catch (_) {}
      }

      function refreshDraftBadge() {
        try {
          var raw = localStorage.getItem(DRAFT_KEY);
          btnRestore.classList.toggle('visible', !!raw);
        } catch (_) {
          btnRestore.classList.remove('visible');
        }
      }

      window.focusQuickNote = function () {
        state.saving = false;
        hintEl.textContent = DEFAULT_HINT;
        hintEl.className = 'hint';
        refreshDraftBadge();
        renderTags();
        requestResize();
        setTimeout(function () {
          if (state.isChecklist) {
            var first = checklistWrap.querySelector('input[type="text"]');
            if (first) first.focus();
          } else {
            bodyEl.focus();
          }
        }, 20);
      };

      window.onQuickNoteSaveError = function (msg) {
        state.saving = false;
        hintEl.textContent = msg || 'Não foi possível salvar no Keep';
        hintEl.className = 'hint warn';
        requestResize();
        setTimeout(function () {
          if (state.isChecklist) {
            var first = checklistWrap.querySelector('input[type="text"]');
            if (first) first.focus();
          } else {
            bodyEl.focus();
          }
        }, 20);
      };

      window.resetQuickNote = function () {
        state.saving = false;
        titleEl.value = '';
        bodyEl.value = '';
        tagInput.value = '';
        state.pinned = false;
        state.tags = [];
        state.items = [{ text: '', checked: false }];
        state.paletteOpen = false;
        state.tagsOpen = false;
        btnPin.classList.remove('active');
        btnPalette.classList.remove('active');
        btnTags.classList.remove('active');
        paletteBar.classList.remove('visible');
        tagsBar.classList.remove('visible');
        renderTags();
        applyColor('default');
        setChecklistMode(false, true);
        hintEl.textContent = DEFAULT_HINT;
        hintEl.className = 'hint';
        refreshDraftBadge();
      };

      window.resetQuickNoteIfSaving = function () {
        if (state.saving) {
          window.resetQuickNote();
        }
      };

      btnRestore.addEventListener('click', function () {
        try {
          var raw = localStorage.getItem(DRAFT_KEY);
          if (!raw) return;
          var d = JSON.parse(raw);
          titleEl.value = d.title || '';
          bodyEl.value = d.body || '';
          state.pinned = !!d.pinned;
          btnPin.classList.toggle('active', state.pinned);
          state.tags = Array.isArray(d.tags) ? d.tags : [];
          if (state.tags.length) {
            state.tagsOpen = true;
            btnTags.classList.add('active');
            tagsBar.classList.add('visible');
          }
          renderTags();
          applyColor(d.colorId || 'default');
          if (d.isChecklist) {
            state.items = Array.isArray(d.items) && d.items.length ? d.items : [{ text: '', checked: false }];
            setChecklistMode(true, true);
          } else {
            setChecklistMode(false, true);
          }
          localStorage.removeItem(DRAFT_KEY);
          refreshDraftBadge();
          requestResize();
        } catch (_) {}
      });

      function submitNote() {
        if (state.saving) return;
        addTagFromInput();
        if (!hasContent()) {
          hintEl.textContent = 'Digite algo antes de salvar';
          hintEl.className = 'hint warn';
          if (state.isChecklist) {
            var first = checklistWrap.querySelector('input[type="text"]');
            if (first) first.focus();
          } else {
            bodyEl.focus();
          }
          return;
        }
        state.saving = true;
        hintEl.textContent = 'Salvando no Keep...';
        hintEl.className = 'hint ok';
        try { localStorage.removeItem(DRAFT_KEY); } catch (_) {}

        var colObj = COLORS.find(function (c) { return c.id === state.colorId; }) || COLORS[0];
        var cleanItems = state.items
          .map(function (i) { return { text: (i.text || '').trim(), checked: !!i.checked }; })
          .filter(function (i) { return i.text.length > 0; });

        setTimeout(function () {
          send({
            action: 'submit',
            title: titleEl.value,
            body: bodyEl.value,
            is_checklist: state.isChecklist && cleanItems.length > 0,
            items: cleanItems,
            color_labels: colObj.labels,
            pinned: state.pinned,
            tags: state.tags
          });
        }, 120);
      }

      function closeModal() {
        saveDiscardedDraftIfAny();
        send({ action: 'close' });
      }

      btnPin.addEventListener('click', function () {
        state.pinned = !state.pinned;
        btnPin.classList.toggle('active', state.pinned);
      });

      btnChecklist.addEventListener('click', function () {
        setChecklistMode(!state.isChecklist, false);
      });

      btnPalette.addEventListener('click', function () {
        state.paletteOpen = !state.paletteOpen;
        btnPalette.classList.toggle('active', state.paletteOpen);
        paletteBar.classList.toggle('visible', state.paletteOpen);
        requestResize();
      });

      btnTags.addEventListener('click', function () {
        state.tagsOpen = !state.tagsOpen;
        btnTags.classList.toggle('active', state.tagsOpen || state.tags.length > 0);
        tagsBar.classList.toggle('visible', state.tagsOpen);
        renderTags();
        requestResize();
        if (state.tagsOpen) {
          send({ action: 'refresh_labels' });
          tagInput.focus();
        }
      });

      btnRefreshLabels.addEventListener('click', function () {
        send({ action: 'refresh_labels' });
        hintEl.textContent = 'Buscando marcadores...';
        hintEl.className = 'hint';
        setTimeout(function () {
          if (!state.saving && hintEl.textContent === 'Buscando marcadores...') {
            hintEl.textContent = DEFAULT_HINT;
          }
        }, 1600);
      });

      tagInput.addEventListener('input', function () {
        renderTags();
      });

      tagInput.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' || e.key === ',') {
          e.preventDefault();
          addTagFromInput();
        }
      });

      bodyEl.addEventListener('input', requestResize);

      titleEl.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
          e.preventDefault();
          if (state.isChecklist) {
            var first = checklistWrap.querySelector('input[type="text"]');
            if (first) first.focus();
          } else {
            bodyEl.focus();
          }
        }
      });

      document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape') {
          e.preventDefault();
          closeModal();
        } else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
          e.preventDefault();
          submitNote();
        }
      });

      document.getElementById('btn-save').addEventListener('click', submitNote);
      document.getElementById('btn-cancel').addEventListener('click', closeModal);
      document.getElementById('btn-close').addEventListener('click', closeModal);
      document.getElementById('btn-open-main').addEventListener('click', function () {
        send({ action: 'open_main' });
      });

      var dragHeader = document.getElementById('drag-header');
      dragHeader.addEventListener('mousedown', function (e) {
        if (e.button === 0 && !e.target.closest('button')) {
          send({ action: 'drag' });
        }
      });
      dragHeader.addEventListener('dblclick', function (e) {
        if (!e.target.closest('button')) {
          send({ action: 'center' });
        }
      });

      window.addEventListener('DOMContentLoaded', function () {
        refreshDraftBadge();
        renderTags();
        bodyEl.focus();
      });
    })();
  </script>
</body>
</html>"#;

#[derive(Deserialize)]
struct ChecklistItemInput {
    text: String,
    checked: bool,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum QuickNoteIpc {
    Submit {
        title: String,
        body: String,
        #[serde(default)]
        is_checklist: bool,
        #[serde(default)]
        items: Vec<ChecklistItemInput>,
        #[serde(default)]
        color_labels: Vec<String>,
        #[serde(default)]
        pinned: bool,
        #[serde(default)]
        tags: Vec<String>,
    },
    RefreshLabels,
    Close,
    OpenMain,
    Drag,
    Center,
    Resize {
        height: f64,
    },
}

const EXTRACT_KEEP_LABELS_JS: &str = r####"(function () {
  try {
    if (window.location.hostname !== 'keep.google.com' || document.readyState !== 'complete') {
      return null;
    }
    var hasShell = !!(
      document.querySelector('[role="navigation"], nav, aside') ||
      document.querySelector('.IZ65Hb-n0tgWb')
    );
    if (!hasShell) {
      return null;
    }

    var seen = Object.create(null);
    var out = [];
    var sysBlocked = {
      'notas': 1, 'notes': 1,
      'lembretes': 1, 'reminders': 1,
      'editar marcadores': 1, 'edit labels': 1, 'editar etiquetas': 1,
      'arquivo': 1, 'archive': 1,
      'lixeira': 1, 'trash': 1, 'bin': 1, 'papelera': 1,
      'marcadores': 1, 'labels': 1, 'etiquetas': 1,
      'configurações': 1, 'configuracoes': 1, 'settings': 1,
      'ajuda e feedback': 1, 'help & feedback': 1, 'ajuda': 1, 'help': 1,
      'enviar feedback': 1, 'send feedback': 1,
      'atalhos do teclado': 1, 'keyboard shortcuts': 1,
      'downloads de aplicativos': 1, 'app downloads': 1,
      'criar novo marcador': 1, 'create new label': 1,
      'licenças de código aberto': 1, 'open-source licenses': 1,
      'privacidade': 1, 'privacy': 1,
      'termos': 1, 'terms': 1,
      'termos de serviço': 1, 'terms of service': 1,
      'política de privacidade': 1, 'privacy policy': 1,
      'ativar tema escuro': 1, 'desativar tema escuro': 1,
      'enable dark theme': 1, 'disable dark theme': 1,
      'menu principal': 1, 'main menu': 1,
      'atualizar': 1, 'refresh': 1,
      'visualização de lista': 1, 'visualização de grade': 1,
      'list view': 1, 'grid view': 1,
      'conta do google': 1, 'google account': 1,
      'google apps': 1, 'google keep': 1,
      'nova lista': 1, 'new list': 1,
      'nova nota com imagem': 1, 'new note with image': 1,
      'nova nota com desenho': 1, 'new note with drawing': 1,
      'remover marcador': 1, 'remove label': 1
    };

    function addLabel(raw) {
      var name = String(raw || '')
        .replace(/[\uE000-\uF8FF\u200B-\u200D\uFEFF]/g, '')
        .replace(/\s+/g, ' ')
        .trim();
      if (!name || name.length > 80) return;
      var key = name.toLowerCase();
      if (sysBlocked[key] || seen[key]) return;
      seen[key] = 1;
      out.push(name);
    }

    function extractTextWithoutIcons(el) {
      if (!el) return '';
      var parts = [];
      var walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
      var node;
      while ((node = walker.nextNode())) {
        var val = (node.nodeValue || '').replace(/[\uE000-\uF8FF\u200B-\u200D\uFEFF]/g, '');
        if (!val || !val.trim()) continue;
        var skip = false;
        var p = node.parentElement;
        while (p && p !== el.parentElement) {
          var tag = (p.tagName || '').toLowerCase();
          if (tag === 'svg' || tag === 'style' || tag === 'script') {
            skip = true;
            break;
          }
          if (p.getAttribute && p.getAttribute('aria-hidden') === 'true') {
            skip = true;
            break;
          }
          var cls = (typeof p.className === 'string' ? p.className : '').toLowerCase();
          if (
            cls.indexOf('material-icons') !== -1 ||
            cls.indexOf('material-symbols') !== -1 ||
            cls.indexOf('google-material') !== -1
          ) {
            skip = true;
            break;
          }
          try {
            var ff = (window.getComputedStyle(p).fontFamily || '').toLowerCase();
            if (ff.indexOf('material') !== -1 || ff.indexOf('glyph') !== -1) {
              skip = true;
              break;
            }
          } catch (_) {}
          p = p.parentElement;
        }
        if (!skip) parts.push(val.trim());
      }
      return parts.join(' ').replace(/\s+/g, ' ').trim();
    }

    // 1. Links diretos para marcadores (#label/...)
    var labelLinks = document.querySelectorAll('a[href*="#label/"], [href*="label/"]');
    for (var i = 0; i < labelLinks.length; i++) {
      var a = labelLinks[i];
      if (a.closest('[role="dialog"]')) continue;
      var txt = extractTextWithoutIcons(a);
      if (!txt) {
        var href = a.getAttribute('href') || '';
        var idx = href.indexOf('label/');
        if (idx !== -1) {
          try {
            txt = decodeURIComponent(href.slice(idx + 6).split(/[?#]/)[0]);
          } catch (_) {}
        }
      }
      addLabel(txt);
    }

    // 2. Itens na barra lateral ([role="navigation"], nav, aside), inclusive roving tabindex (-1)
    var navRoots = document.querySelectorAll('[role="navigation"], nav, aside');
    for (var r = 0; r < navRoots.length; r++) {
      var nav = navRoots[r];
      if (nav.closest('[role="dialog"]')) continue;
      var navItems = nav.querySelectorAll('[role="menuitem"], [role="button"], [role="link"], a, li, div[tabindex], span[tabindex]');
      for (var j = 0; j < navItems.length; j++) {
        var el = navItems[j];
        if (el.closest('[role="dialog"]')) continue;
        // Ignora containers que apenas agrupam outros itens clicáveis
        if (el.querySelector('[role="menuitem"], a, li')) continue;
        var h = (el.getAttribute('href') || '').toLowerCase();
        if (h && (h.endsWith('#home') || h.endsWith('#reminders') || h.endsWith('#archive') || h.endsWith('#trash') || h.endsWith('#settings'))) {
          continue;
        }
        var cleanTxt = extractTextWithoutIcons(el);
        var aria = (el.getAttribute('aria-label') || '').trim();
        if (cleanTxt) {
          addLabel(cleanTxt);
        } else if (aria && aria.indexOf('\n') === -1 && aria.toLowerCase().indexOf('menu') === -1) {
          addLabel(aria.replace(/^(marcador|label)\s*:\s*/i, ''));
        }
      }
    }

    // 3. Chips de marcadores visíveis nos cartões de notas
    var chipElements = document.querySelectorAll('.Q0hgme-LgbsSe-Bz112c, [aria-label*="Remover marcador" i], [aria-label*="Remove label" i]');
    for (var k = 0; k < chipElements.length; k++) {
      var chip = chipElements[k];
      if (chip.closest('[role="dialog"]')) continue;
      var al = chip.getAttribute('aria-label') || '';
      var cleaned = al
        .replace(/^remover marcador\s*/i, '')
        .replace(/^remove label\s*/i, '')
        .trim();
      if (cleaned) {
        addLabel(cleaned);
      } else {
        var chipTxt = extractTextWithoutIcons(chip) || extractTextWithoutIcons(chip.parentElement);
        if (chipTxt) addLabel(chipTxt);
      }
    }

    out.sort(function (a, b) {
      return a.localeCompare(b, undefined, { sensitivity: 'base' });
    });
    return { ready: true, labels: out };
  } catch (_) {
    return null;
  }
})()"####;

fn build_submit_note_js(
    submit_id: u64,
    title: &str,
    body: &str,
    is_checklist: bool,
    items: &[ChecklistItemInput],
    color_labels: &[String],
    pinned: bool,
    tags: &[String],
) -> String {
    let title_json = serde_json::to_string(title).unwrap_or_else(|_| "\"\"".to_string());
    let body_json = serde_json::to_string(body).unwrap_or_else(|_| "\"\"".to_string());
    let items_json = serde_json::to_string(
        &items
            .iter()
            .map(|i| serde_json::json!({ "text": i.text, "checked": i.checked }))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    let colors_json = serde_json::to_string(color_labels).unwrap_or_else(|_| "[]".to_string());
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());

    format!(
        r#"(function (cfg) {{
  var attempts = 0;
  var maxAttempts = 45;
  var expandSent = false;
  var hasLayout = window.innerWidth > 0 && window.innerHeight > 0;

  function setStatus(done, ok, stage, detail) {{
    window.__kdSubmitStatus = {{
      id: cfg.submitId,
      done: !!done,
      ok: !!ok,
      stage: stage || '',
      detail: detail || ''
    }};
  }}

  setStatus(false, false, 'init', 'Iniciando envio');

  function isElementVisible(el) {{
    if (!el) return false;
    var cur = el;
    while (cur && cur.nodeType === 1) {{
      if (cur.getAttribute && cur.getAttribute('aria-hidden') === 'true') return false;
      var st = window.getComputedStyle(cur);
      if (st.display === 'none' || st.visibility === 'hidden' || st.opacity === '0') return false;
      cur = cur.parentElement;
    }}
    if (hasLayout) {{
      var rect = el.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return false;
    }}
    return true;
  }}

  function isFieldDisplayed(el) {{
    if (!el || el.nodeType !== 1) return false;
    if (el.getAttribute && el.getAttribute('aria-hidden') === 'true') return false;
    var cur = el;
    while (cur && cur.nodeType === 1) {{
      var st = window.getComputedStyle(cur);
      if (st.display === 'none' || st.visibility === 'hidden') return false;
      cur = cur.parentElement;
    }}
    return true;
  }}

  function dispatchClick(el) {{
    if (!el) return;
    try {{ el.focus(); }} catch (_) {{}}
    try {{
      el.dispatchEvent(new PointerEvent('pointerdown', {{ bubbles: true, cancelable: true }}));
    }} catch (_) {{}}
    try {{
      el.dispatchEvent(new MouseEvent('mousedown', {{ bubbles: true, cancelable: true, view: window }}));
    }} catch (_) {{}}
    try {{
      el.dispatchEvent(new PointerEvent('pointerup', {{ bubbles: true, cancelable: true }}));
    }} catch (_) {{}}
    try {{
      el.dispatchEvent(new MouseEvent('mouseup', {{ bubbles: true, cancelable: true, view: window }}));
    }} catch (_) {{}}
    try {{
      el.click();
    }} catch (_) {{}}
  }}

  function setNativeInputValue(input, value) {{
    try {{
      var proto = input.tagName === 'TEXTAREA'
        ? window.HTMLTextAreaElement.prototype
        : window.HTMLInputElement.prototype;
      var desc = Object.getOwnPropertyDescriptor(proto, 'value');
      if (desc && desc.set) {{
        desc.set.call(input, value);
      }} else {{
        input.value = value;
      }}
    }} catch (_) {{
      input.value = value;
    }}
    try {{
      input.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: value }}));
    }} catch (_) {{
      input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    }}
    input.dispatchEvent(new KeyboardEvent('keyup', {{ key: ' ', bubbles: true }}));
    input.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}

  function findButtonByLabels(root, patterns) {{
    var nodes = (root || document).querySelectorAll('[role="button"], button, [role="menuitem"]');
    for (var p = 0; p < patterns.length; p++) {{
      var target = patterns[p].toLowerCase();
      for (var i = 0; i < nodes.length; i++) {{
        if (nodes[i].closest('[role="dialog"]') && root === document) continue;
        var al = (nodes[i].getAttribute('aria-label') || '').toLowerCase();
        var tt = (nodes[i].getAttribute('data-tooltip') || '').toLowerCase();
        var tl = (nodes[i].getAttribute('title') || '').toLowerCase();
        var tx = (nodes[i].textContent || '').trim().toLowerCase();
        if ((al.indexOf(target) !== -1 || tt.indexOf(target) !== -1 || tl.indexOf(target) !== -1 || tx === target) && isElementVisible(nodes[i])) {{
          return nodes[i];
        }}
      }}
    }}
    return null;
  }}

  function findCollapsedComposerTrigger() {{
    var selectors = [
      '[aria-label*="Criar uma nota" i]',
      '[aria-label*="Take a note" i]',
      '[aria-label*="Adicionar uma nota" i]',
      '[data-tooltip*="Criar uma nota" i]',
      '[data-tooltip*="Take a note" i]'
    ];
    for (var s = 0; s < selectors.length; s++) {{
      var list = document.querySelectorAll(selectors[s]);
      for (var i = 0; i < list.length; i++) {{
        if (!list[i].closest('[role="dialog"]') && !list[i].closest('header') && isElementVisible(list[i])) {{
          return list[i];
        }}
      }}
    }}

    var textEls = document.querySelectorAll('div, span, p');
    for (var a = 0; a < textEls.length; a++) {{
      var el = textEls[a];
      if (el.closest('[role="dialog"]') || el.closest('header') || !isElementVisible(el)) continue;
      if (el.children.length > 0) continue;
      var t = (el.textContent || '').replace(/\u2026/g, '...').replace(/\s+/g, ' ').trim().toLowerCase();
      if (t === 'criar uma nota...' || t === 'criar uma nota' || t === 'take a note...' || t === 'take a note') {{
        return el.closest('[role="textbox"], [contenteditable="true"], .IZ65Hb-TBnied, .IZ65Hb-n0tgWb') || el.parentElement || el;
      }}
    }}

    var newListBtn = findButtonByLabels(document, ['nova lista', 'new list']);
    if (newListBtn) {{
      var card = newListBtn.closest('.IZ65Hb-n0tgWb, .IZ65Hb-TBnied') || (newListBtn.parentElement && newListBtn.parentElement.parentElement);
      if (card) {{
        var box = card.querySelector('[contenteditable="true"], [role="textbox"]');
        if (box && isElementVisible(box)) return box;
        return card;
      }}
    }}
    return null;
  }}

  function collectEditableBoxes(root) {{
    if (!root) return [];
    var candidates = root.querySelectorAll(
      '[contenteditable="true"], [contenteditable=""], [contenteditable="plaintext-only"], textarea'
    );
    var out = [];
    for (var i = 0; i < candidates.length; i++) {{
      var cand = candidates[i];
      if (!cand || cand.nodeType !== 1) continue;
      if (cand.getAttribute('role') === 'button') continue;
      if (cand.closest('[role="dialog"]') && root === document) continue;
      if (cand.tagName !== 'TEXTAREA' && cand.isContentEditable !== true) continue;
      if (!isFieldDisplayed(cand)) continue;
      var nested = false;
      for (var u = 0; u < out.length; u++) {{
        if (out[u] === cand || out[u].contains(cand)) {{
          nested = true;
          break;
        }}
      }}
      if (!nested) {{
        out.push(cand);
      }}
    }}
    return out;
  }}

  function looksLikeTitleBox(box) {{
    if (!box) return false;
    if (box.classList && box.classList.contains('r4nke-YPqjbf')) return true;
    var bal = (box.getAttribute('aria-label') || '').toLowerCase();
    var ph = (box.getAttribute('data-placeholder') || box.getAttribute('placeholder') || '').toLowerCase();
    if (
      bal.indexOf('título') !== -1 ||
      bal.indexOf('titulo') !== -1 ||
      bal.indexOf('title') !== -1 ||
      ph.indexOf('título') !== -1 ||
      ph.indexOf('titulo') !== -1 ||
      ph.indexOf('title') !== -1
    ) {{
      return true;
    }}
    var parentTxt = (box.parentElement ? box.parentElement.textContent || '' : '').trim().toLowerCase();
    return parentTxt === 'título' || parentTxt === 'titulo' || parentTxt === 'title';
  }}

  function looksLikeBodyBox(box) {{
    if (!box) return false;
    if (box.classList && box.classList.contains('h1U9Be-YPqjbf')) return true;
    var bal = (box.getAttribute('aria-label') || '').toLowerCase();
    var ph = (box.getAttribute('data-placeholder') || box.getAttribute('placeholder') || '').toLowerCase();
    var combined = bal + ' ' + ph;
    if (
      combined.indexOf('criar uma nota') !== -1 ||
      combined.indexOf('take a note') !== -1 ||
      combined.indexOf('adicionar uma nota') !== -1 ||
      combined.indexOf('añadir una nota') !== -1 ||
      combined.indexOf('crear una nota') !== -1 ||
      combined.indexOf('notiz schreiben') !== -1 ||
      bal === 'nota' ||
      bal === 'note'
    ) {{
      return true;
    }}
    var parentTxt = (box.parentElement ? box.parentElement.textContent || '' : '')
      .replace(/\u2026/g, '...')
      .replace(/\s+/g, ' ')
      .trim()
      .toLowerCase();
    return (
      parentTxt === 'criar uma nota...' ||
      parentTxt === 'criar uma nota' ||
      parentTxt === 'take a note...' ||
      parentTxt === 'take a note'
    );
  }}

  function countCloseButtons(root) {{
    if (!root) return 0;
    var nodes = root.querySelectorAll('[role="button"], button');
    var count = 0;
    for (var i = 0; i < nodes.length; i++) {{
      if (nodes[i].closest('[role="dialog"]')) continue;
      var al = (nodes[i].getAttribute('aria-label') || '').toLowerCase();
      var tt = (nodes[i].getAttribute('data-tooltip') || '').toLowerCase();
      var tx = (nodes[i].textContent || '').trim().toLowerCase();
      var isClose = tx === 'fechar' || tx === 'close' || tx === 'cerrar' || tx === 'fermer'
        || al === 'fechar' || al === 'close' || tt === 'fechar' || tt === 'close';
      if (isClose && isElementVisible(nodes[i])) {{
        count++;
      }}
    }}
    return count;
  }}

  function findBodyFallback(root, titleBox) {{
    if (!root) return null;
    var selectors = [
      '.h1U9Be-YPqjbf',
      '[aria-label*="Criar uma nota" i]',
      '[aria-label*="Take a note" i]',
      '[role="textbox"]:not([role="button"])',
      'textarea'
    ];
    var bestEditable = null;
    var bestAny = null;
    for (var s = 0; s < selectors.length; s++) {{
      var nodes = root.querySelectorAll(selectors[s]);
      for (var i = 0; i < nodes.length; i++) {{
        var n = nodes[i];
        if (!n || n === titleBox) continue;
        if (titleBox && (n.contains(titleBox) || titleBox.contains(n))) continue;
        if (n.getAttribute('role') === 'button') continue;
        if (looksLikeTitleBox(n)) continue;
        if (!isFieldDisplayed(n)) continue;
        var innerEd = n.querySelector && n.querySelector('[contenteditable="true"], [contenteditable=""], textarea');
        var candidate = (innerEd && innerEd !== titleBox && isFieldDisplayed(innerEd)) ? innerEd : n;
        if (candidate.tagName === 'TEXTAREA' || candidate.isContentEditable === true) {{
          if (!bestEditable) bestEditable = candidate;
        }} else if (!bestAny) {{
          bestAny = candidate;
        }}
      }}
      if (bestEditable) return bestEditable;
    }}
    return bestAny;
  }}

  function describeComposerDebug(root) {{
    try {{
      if (!root) return 'sem root';
      var all = root.querySelectorAll('[contenteditable], [role="textbox"], textarea, .h1U9Be-YPqjbf, .r4nke-YPqjbf');
      var parts = [];
      for (var i = 0; i < all.length && parts.length < 6; i++) {{
        var el = all[i];
        var cls = (typeof el.className === 'string' ? el.className : '').split(/\s+/).slice(0, 2).join('.');
        var ce = el.getAttribute('contenteditable');
        var aria = el.getAttribute('aria-label') || '';
        var ah = el.getAttribute('aria-hidden') || '';
        parts.push(el.tagName + '(' + cls + ',ce=' + ce + ',ed=' + !!el.isContentEditable + ',ah=' + ah + ',al=' + aria + ')');
      }}
      return 'n=' + all.length + ' ' + parts.join('; ');
    }} catch (_) {{
      return 'debug-err';
    }}
  }}

  function findExpandedComposer() {{
    var closeNodes = document.querySelectorAll('[role="button"], button');
    for (var c = 0; c < closeNodes.length; c++) {{
      var btn = closeNodes[c];
      if (btn.closest('[role="dialog"]')) continue;
      var al = (btn.getAttribute('aria-label') || '').toLowerCase();
      var tt = (btn.getAttribute('data-tooltip') || '').toLowerCase();
      var tx = (btn.textContent || '').trim().toLowerCase();
      var isClose = tx === 'fechar' || tx === 'close' || tx === 'cerrar' || tx === 'fermer'
        || al === 'fechar' || al === 'close' || tt === 'fechar' || tt === 'close';
      if (!isClose || !isElementVisible(btn)) continue;

      var cur = btn.parentElement;
      for (var d = 0; d < 12 && cur && cur !== document.body; d++) {{
        var visibleBoxes = collectEditableBoxes(cur);
        if (visibleBoxes.length >= 1) {{
          var chosenRoot = cur;
          var chosenBoxes = visibleBoxes;
          if (chosenBoxes.length === 1) {{
            var probeParent = cur.parentElement;
            for (var extra = 0; extra < 4 && probeParent && probeParent !== document.body; extra++) {{
              if (countCloseButtons(probeParent) > 1) break;
              var parentBoxes = collectEditableBoxes(probeParent);
              if (parentBoxes.length >= 2 && parentBoxes.length <= 4) {{
                chosenRoot = probeParent;
                chosenBoxes = parentBoxes;
                break;
              }}
              probeParent = probeParent.parentElement;
            }}
          }}

          var titleBox = null;
          var bodyBox = null;
          for (var i = 0; i < chosenBoxes.length; i++) {{
            var box = chosenBoxes[i];
            if (!titleBox && looksLikeTitleBox(box) && !looksLikeBodyBox(box)) {{
              titleBox = box;
            }} else if (!bodyBox && looksLikeBodyBox(box) && !looksLikeTitleBox(box)) {{
              bodyBox = box;
            }}
          }}
          if (chosenBoxes.length === 1) {{
            if (!titleBox && !bodyBox) {{
              if (looksLikeTitleBox(chosenBoxes[0])) {{
                titleBox = chosenBoxes[0];
              }} else {{
                bodyBox = chosenBoxes[0];
              }}
            }}
          }} else {{
            for (var j = 0; j < chosenBoxes.length; j++) {{
              var rem = chosenBoxes[j];
              if (rem === titleBox || rem === bodyBox) continue;
              if (!titleBox && !looksLikeBodyBox(rem)) {{
                titleBox = rem;
              }} else if (!bodyBox && !looksLikeTitleBox(rem)) {{
                bodyBox = rem;
              }}
            }}
          }}
          if (!bodyBox) {{
            var searchRoot = chosenRoot;
            var cardRoot = chosenRoot.closest && chosenRoot.closest('.IZ65Hb-TBnied, .IZ65Hb-n0tgWb');
            if (cardRoot && countCloseButtons(cardRoot) <= 1) {{
              searchRoot = cardRoot;
            }}
            bodyBox = findBodyFallback(searchRoot, titleBox);
          }}
          return {{ root: chosenRoot, titleBox: titleBox, bodyBox: bodyBox, closeBtn: btn }};
        }}
        cur = cur.parentElement;
      }}
    }}
    return null;
  }}

  function containsExpectedText(el, expected) {{
    if (!el) return false;
    var norm = String(expected || '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
    var nonEmptyLines = norm.split('\n').map(function (l) {{ return l.trim(); }}).filter(Boolean);
    if (!nonEmptyLines.length) return true;
    var firstLine = nonEmptyLines[0];
    var lastLine = nonEmptyLines[nonEmptyLines.length - 1];
    var firstProbe = firstLine.slice(0, Math.min(20, firstLine.length)).replace(/[\u00a0\s]+/g, ' ');
    var lastProbe = lastLine.slice(0, Math.min(20, lastLine.length)).replace(/[\u00a0\s]+/g, ' ');
    var rawGot = (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT')
      ? (el.value || '')
      : (el.innerText || el.textContent || '');
    var got = rawGot.replace(/\r/g, '').replace(/[\u00a0\s]+/g, ' ');
    return got.indexOf(firstProbe) !== -1 && got.indexOf(lastProbe) !== -1;
  }}

  function insertIntoEditable(el, value) {{
    if (!el) return false;
    if (!value) return true;
    var normalizedExpected = String(value).replace(/\r\n/g, '\n').replace(/\r/g, '\n');

    dispatchClick(el);
    try {{ el.focus(); }} catch (_) {{}}

    if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {{
      setNativeInputValue(el, normalizedExpected);
      return containsExpectedText(el, normalizedExpected);
    }}

    function clearEditableSelection() {{
      var sel = window.getSelection();
      var range = document.createRange();
      range.selectNodeContents(el);
      sel.removeAllRanges();
      sel.addRange(range);
      if (typeof document.execCommand === 'function' && (el.textContent || '').length > 0) {{
        document.execCommand('selectAll', false, null);
        document.execCommand('delete', false, null);
      }} else {{
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
      }}
    }}

    function notifyInput() {{
      try {{
        el.dispatchEvent(new InputEvent('input', {{
          bubbles: true,
          inputType: 'insertText',
          data: normalizedExpected
        }}));
      }} catch (_) {{
        el.dispatchEvent(new Event('input', {{ bubbles: true }}));
      }}
      el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }}

    try {{
      clearEditableSelection();
      if (typeof document.execCommand === 'function') {{
        var ok = document.execCommand('insertText', false, normalizedExpected);
        if (ok && containsExpectedText(el, normalizedExpected)) {{
          notifyInput();
          return true;
        }}
        if (normalizedExpected.indexOf('\n') !== -1) {{
          clearEditableSelection();
          var splitLines = normalizedExpected.split('\n');
          var lineOk = true;
          for (var li = 0; li < splitLines.length; li++) {{
            if (li > 0) {{
              document.execCommand('insertLineBreak', false, null);
            }}
            if (splitLines[li].length > 0) {{
              if (!document.execCommand('insertText', false, splitLines[li])) {{
                lineOk = false;
              }}
            }}
          }}
          if (lineOk && containsExpectedText(el, normalizedExpected)) {{
            notifyInput();
            return true;
          }}
        }}
      }}
    }} catch (_) {{}}

    try {{
      clearEditableSelection();
      var dt = new DataTransfer();
      dt.setData('text/plain', normalizedExpected);
      el.dispatchEvent(new ClipboardEvent('paste', {{
        bubbles: true,
        cancelable: true,
        clipboardData: dt
      }}));
      if (containsExpectedText(el, normalizedExpected)) {{
        notifyInput();
        return true;
      }}
    }} catch (_) {{}}

    while (el.firstChild) {{
      el.removeChild(el.firstChild);
    }}
    var lines = normalizedExpected.split('\n');
    for (var i = 0; i < lines.length; i++) {{
      if (i > 0) {{
        el.appendChild(document.createElement('br'));
      }}
      if (lines[i].length > 0) {{
        el.appendChild(document.createTextNode(lines[i]));
      }}
    }}
    try {{
      el.dispatchEvent(new InputEvent('beforeinput', {{
        bubbles: true,
        cancelable: true,
        inputType: 'insertText',
        data: normalizedExpected
      }}));
    }} catch (_) {{}}
    notifyInput();
    try {{
      el.dispatchEvent(new CompositionEvent('compositionend', {{ bubbles: true, data: normalizedExpected }}));
    }} catch (_) {{}}
    el.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'a', code: 'KeyA', bubbles: true }}));
    el.dispatchEvent(new KeyboardEvent('keyup', {{ key: 'a', code: 'KeyA', bubbles: true }}));
    return containsExpectedText(el, normalizedExpected);
  }}

  function findCreateLabelButton(tagName) {{
    var target = tagName.toLowerCase();
    var candidates = document.querySelectorAll('[role="menuitem"], [role="button"], div[tabindex], li');
    for (var i = 0; i < candidates.length; i++) {{
      var el = candidates[i];
      if (!isElementVisible(el)) continue;
      var txt = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim().toLowerCase();
      if (!txt) continue;
      if ((txt.indexOf('criar ') === 0 || txt.indexOf('create ') === 0) && txt.indexOf(target) !== -1) {{
        return el;
      }}
    }}
    return null;
  }}

  function applyColorAndPinAndClose(comp) {{
    var root = comp.root;
    var refEl = comp.bodyBox || comp.titleBox;
    setStatus(false, false, 'styling', 'Aplicando opções e fechando nota');

    if (cfg.pinned) {{
      var pinBtn = findButtonByLabels(root, ['fixar nota', 'pin note']);
      if (pinBtn && pinBtn.getAttribute('aria-pressed') !== 'true') {{
        dispatchClick(pinBtn);
      }}
    }}

    function finishClose() {{
      setStatus(false, false, 'closing', 'Fechando editor para confirmar salvamento');
      var closeBtn = findButtonByLabels(root, ['fechar', 'close', 'cerrar', 'fermer']) || comp.closeBtn;
      if (closeBtn && isElementVisible(closeBtn)) {{
        dispatchClick(closeBtn);
        setTimeout(function () {{
          if (!findExpandedComposer()) {{
            setStatus(true, true, 'saved', 'Nota salva no Google Keep');
            return;
          }}
          dispatchClick(closeBtn);
          if (refEl) {{
            try {{ refEl.blur(); }} catch (_) {{}}
          }}
          setTimeout(function () {{
            if (!findExpandedComposer()) {{
              setStatus(true, true, 'saved', 'Nota salva no Google Keep');
            }} else {{
              document.dispatchEvent(new KeyboardEvent('keydown', {{
                key: 'Escape',
                code: 'Escape',
                keyCode: 27,
                which: 27,
                bubbles: true
              }}));
              setTimeout(function () {{
                if (!findExpandedComposer()) {{
                  setStatus(true, true, 'saved', 'Nota salva no Google Keep');
                }} else {{
                  setStatus(true, false, 'closing', 'Editor do Keep permaneceu aberto após clicar em Fechar');
                }}
              }}, 350);
            }}
          }}, 320);
        }}, 300);
      }} else {{
        setStatus(true, false, 'closing', 'Botão Fechar do editor não encontrado');
      }}
    }}

    function doTagsAndClose() {{
      if (cfg.tags && cfg.tags.length > 0) {{
        var moreBtn = findButtonByLabels(root, ['mais', 'more']);
        if (moreBtn) {{
          dispatchClick(moreBtn);
          setTimeout(function () {{
            var addLabelItem = findButtonByLabels(document, ['adicionar marcador', 'alterar marcadores', 'add label', 'change labels', 'marcadores', 'labels']);
            if (addLabelItem) {{
              dispatchClick(addLabelItem);
              setTimeout(function () {{
                var idx = 0;
                function nextTag() {{
                  if (idx >= cfg.tags.length) {{
                    if (refEl) dispatchClick(refEl);
                    setTimeout(finishClose, 180);
                    return;
                  }}
                  var t = String(cfg.tags[idx++] || '').trim();
                  if (!t) {{
                    nextTag();
                    return;
                  }}
                  var searchInput = document.querySelector(
                    'input[aria-label*="marcador" i], input[aria-label*="label" i], input[placeholder*="marcador" i], input[placeholder*="label" i], input[placeholder*="digite" i], input[maxlength="50"]'
                  );
                  if (searchInput && isElementVisible(searchInput)) {{
                    searchInput.focus();
                    setNativeInputValue(searchInput, t);
                    setTimeout(function () {{
                      var cbs = document.querySelectorAll('[role="menuitemcheckbox"], [role="checkbox"]');
                      var clicked = false;
                      var exactTarget = t.toLowerCase();
                      for (var k = 0; k < cbs.length; k++) {{
                        if (!isElementVisible(cbs[k])) continue;
                        var labelTxt = (cbs[k].innerText || cbs[k].textContent || '').replace(/\s+/g, ' ').trim().toLowerCase();
                        if (labelTxt === exactTarget) {{
                          if (cbs[k].getAttribute('aria-checked') !== 'true') {{
                            dispatchClick(cbs[k]);
                          }}
                          clicked = true;
                          break;
                        }}
                      }}
                      if (!clicked) {{
                        var createBtn = findCreateLabelButton(t);
                        if (createBtn) {{
                          dispatchClick(createBtn);
                        }}
                      }}
                      setTimeout(function () {{
                        setNativeInputValue(searchInput, '');
                        setTimeout(nextTag, 140);
                      }}, 160);
                    }}, 180);
                  }} else {{
                    setTimeout(finishClose, 180);
                  }}
                }}
                nextTag();
              }}, 220);
              return;
            }}
            setTimeout(finishClose, 180);
          }}, 220);
          return;
        }}
      }}
      finishClose();
    }}

    if (cfg.colorLabels && cfg.colorLabels.length > 0) {{
      var colorBtn = findButtonByLabels(root, ['opções de plano de fundo', 'background options', 'alterar cor', 'change color']);
      if (colorBtn) {{
        dispatchClick(colorBtn);
        setTimeout(function () {{
          var swatch = findButtonByLabels(document, cfg.colorLabels);
          if (swatch) dispatchClick(swatch);
          setTimeout(doTagsAndClose, 180);
        }}, 180);
        return;
      }}
    }}
    setTimeout(doTagsAndClose, 140);
  }}

  function populateChecklistItems(comp) {{
    setStatus(false, false, 'populating', 'Preenchendo itens da lista');
    var titleBox = comp.titleBox;
    var cleanTitle = (cfg.title || '').trim();
    if (cleanTitle && titleBox) {{
      insertIntoEditable(titleBox, cleanTitle);
    }}

    var idx = 0;
    function insertNextItem(retryCount) {{
      if (idx >= cfg.items.length) {{
        setTimeout(function () {{
          var latest = findExpandedComposer() || comp;
          applyColorAndPinAndClose(latest);
        }}, 200);
        return;
      }}

      var item = cfg.items[idx];
      var inputs = collectEditableBoxes(comp.root);
      var targetInput = null;
      var lastInput = null;
      for (var i = 0; i < inputs.length; i++) {{
        if (inputs[i] !== titleBox) {{
          lastInput = inputs[i];
          if (!(inputs[i].textContent || '').trim()) {{
            targetInput = inputs[i];
            break;
          }}
        }}
      }}

      if (!targetInput && (retryCount || 0) < 4) {{
        if (lastInput) {{
          lastInput.focus();
          lastInput.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
          lastInput.dispatchEvent(new KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
        }}
        setTimeout(function () {{ insertNextItem((retryCount || 0) + 1); }}, 120);
        return;
      }}

      idx++;
      if (targetInput) {{
        insertIntoEditable(targetInput, item.text);
        if (item.checked) {{
          var row = targetInput.closest('[role="listitem"]') || (targetInput.parentElement && targetInput.parentElement.parentElement);
          var cb = row && row.querySelector('[role="checkbox"]');
          if (cb && cb.getAttribute('aria-checked') !== 'true') {{
            dispatchClick(cb);
          }}
        }}
        targetInput.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
        targetInput.dispatchEvent(new KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
      }}
      setTimeout(function () {{ insertNextItem(0); }}, 110);
    }}

    setTimeout(function () {{ insertNextItem(0); }}, 140);
  }}

  function step() {{
    attempts++;
    if (attempts > maxAttempts) {{
      var hadTrigger = !!findCollapsedComposerTrigger();
      var hadExpanded = !!findExpandedComposer();
      setStatus(
        true,
        false,
        'expanding',
        'Tempo esgotado no Keep (trigger: ' + hadTrigger + ', aberto: ' + hadExpanded + ')'
      );
      return;
    }}

    if (window.location.hostname !== 'keep.google.com') {{
      setStatus(true, false, 'auth', 'Keep não está carregado (faça login primeiro)');
      return;
    }}

    if (document.readyState !== 'complete') {{
      setStatus(false, false, 'waiting_ready', 'Aguardando Keep carregar');
      setTimeout(step, 220);
      return;
    }}

    var openDialog = document.querySelector('[role="dialog"]');
    if (openDialog && isElementVisible(openDialog)) {{
      document.dispatchEvent(
        new KeyboardEvent('keydown', {{
          key: 'Escape',
          code: 'Escape',
          keyCode: 27,
          which: 27,
          bubbles: true
        }})
      );
      setTimeout(step, 220);
      return;
    }}

    if (window.location.hash && window.location.hash !== '#home') {{
      window.location.hash = '#home';
      expandSent = false;
      setTimeout(step, 350);
      return;
    }}

    var comp = findExpandedComposer();
    if (comp) {{
      if (cfg.isChecklist && cfg.items && cfg.items.length > 0) {{
        var hasCheckboxes = !!comp.root.querySelector('[role="checkbox"]');
        if (!hasCheckboxes) {{
          var moreBtn = findButtonByLabels(comp.root, ['mais', 'more']);
          if (moreBtn) {{
            dispatchClick(moreBtn);
            setTimeout(function () {{
              var showCb = findButtonByLabels(document, ['mostrar caixas de seleção', 'show checkboxes']);
              if (showCb) dispatchClick(showCb);
              setTimeout(function () {{
                populateChecklistItems(findExpandedComposer() || comp);
              }}, 200);
            }}, 180);
            return;
          }}
        }}
        populateChecklistItems(comp);
        return;
      }}

      setStatus(false, false, 'populating', 'Inserindo título e conteúdo');
      var cleanTitle = (cfg.title || '').trim();
      var cleanBody = cfg.isChecklist && cfg.items && cfg.items.length
        ? cfg.items.map(function (i) {{ return i.text; }}).join('\n')
        : (cfg.body || '').trimEnd();

      if (cleanBody && (!comp.bodyBox || comp.bodyBox === comp.titleBox)) {{
        if (attempts < 18) {{
          setTimeout(step, 200);
          return;
        }}
        setStatus(
          true,
          false,
          'populating',
          'Campo de conteúdo não encontrado no Keep (' + describeComposerDebug(comp.root) + ')'
        );
        return;
      }}

      if (
        cleanBody &&
        comp.bodyBox &&
        comp.bodyBox.tagName !== 'TEXTAREA' &&
        comp.bodyBox.isContentEditable !== true
      ) {{
        dispatchClick(comp.bodyBox);
        try {{ comp.bodyBox.focus(); }} catch (_) {{}}
        if (attempts < 20) {{
          setTimeout(step, 200);
          return;
        }}
        setStatus(
          true,
          false,
          'populating',
          'Campo de conteúdo não ficou editável (' + describeComposerDebug(comp.root) + ')'
        );
        return;
      }}

      if (!comp.bodyBox && !comp.titleBox) {{
        setStatus(true, false, 'populating', 'Campos de edição do Keep não encontrados');
        return;
      }}

      var targetBodyBox = comp.bodyBox;
      var targetTitleBox = comp.titleBox || (!cleanBody ? comp.bodyBox : null);

      function finishPopulate() {{
        var bodyVerified = !cleanBody || (targetBodyBox && containsExpectedText(targetBodyBox, cleanBody));
        if (cleanBody && !bodyVerified && targetBodyBox) {{
          insertIntoEditable(targetBodyBox, cleanBody);
          bodyVerified = containsExpectedText(targetBodyBox, cleanBody);
        }}
        var titleVerified = !cleanTitle || !targetTitleBox || containsExpectedText(targetTitleBox, cleanTitle);
        if (cleanTitle && !titleVerified && targetTitleBox && targetTitleBox !== targetBodyBox) {{
          insertIntoEditable(targetTitleBox, cleanTitle);
          titleVerified = containsExpectedText(targetTitleBox, cleanTitle);
        }}
        if ((cleanBody && !bodyVerified) || (cleanTitle && targetTitleBox && !titleVerified)) {{
          setStatus(
            true,
            false,
            'populating',
            'O editor do Keep não manteve o texto (b=' + bodyVerified + ', t=' + titleVerified + ')'
          );
          return;
        }}
        var latest = findExpandedComposer() || comp;
        applyColorAndPinAndClose(latest);
      }}

      function insertTitleStep() {{
        if (cleanTitle && targetTitleBox && targetTitleBox !== targetBodyBox) {{
          insertIntoEditable(targetTitleBox, cleanTitle);
          setTimeout(finishPopulate, 180);
        }} else if (cleanTitle && targetTitleBox && !cleanBody) {{
          insertIntoEditable(targetTitleBox, cleanTitle);
          setTimeout(finishPopulate, 180);
        }} else {{
          setTimeout(finishPopulate, 160);
        }}
      }}

      if (cleanBody && targetBodyBox) {{
        insertIntoEditable(targetBodyBox, cleanBody);
        setTimeout(insertTitleStep, 160);
      }} else {{
        insertTitleStep();
      }}
      return;
    }}

    setStatus(false, false, 'expanding', 'Abrindo campo "Criar uma nota..."');
    if (cfg.isChecklist && cfg.items && cfg.items.length > 0) {{
      var newListBtn = findButtonByLabels(document, ['nova lista', 'new list']);
      if (newListBtn) {{
        dispatchClick(newListBtn);
        setTimeout(step, 240);
        return;
      }}
    }}

    var trigger = findCollapsedComposerTrigger();
    if (trigger) {{
      dispatchClick(trigger);
      var innerBox = trigger.querySelector && trigger.querySelector('[contenteditable="true"], [role="textbox"]');
      if (innerBox && innerBox !== trigger) {{
        dispatchClick(innerBox);
      }}
      if (expandSent) {{
        document.dispatchEvent(new KeyboardEvent('keydown', {{
          key: 'c',
          code: 'KeyC',
          keyCode: 67,
          which: 67,
          bubbles: true
        }}));
      }}
      expandSent = true;
    }} else if (!expandSent) {{
      document.dispatchEvent(new KeyboardEvent('keydown', {{
        key: 'c',
        code: 'KeyC',
        keyCode: 67,
        which: 67,
        bubbles: true
      }}));
      expandSent = true;
    }}
    setTimeout(step, 240);
  }}

  step();
}})({{
  submitId: {submit_id},
  title: {title_json},
  body: {body_json},
  isChecklist: {is_checklist},
  items: {items_json},
  colorLabels: {colors_json},
  pinned: {pinned},
  tags: {tags_json}
}});"#
    )
}

fn build_main_ui_js(theme_preset: &str, compact_mode: bool) -> String {
    let theme_json =
        serde_json::to_string(theme_preset).unwrap_or_else(|_| "\"default\"".to_string());
    format!(
        r#"(function (themeName, compactMode) {{
  try {{
    if (window.location.hostname !== 'keep.google.com') return;

    var oldBar = document.getElementById('kd-filter-bar');
    if (oldBar) oldBar.remove();

    var themeVars = '';
    if (themeName === 'amoled') {{
      themeVars = 'body, html, header[role="banner"], .gb_Id, .gb_Kd, [role="navigation"] {{ background-color: #000000 !important; }} '
        + '.IZ65Hb-n0tgWb {{ border-color: rgba(255,255,255,0.12) !important; }}';
    }} else if (themeName === 'nord') {{
      themeVars = 'body, html {{ background-color: #2e3440 !important; }} '
        + 'header[role="banner"], .gb_Id, .gb_Kd, [role="navigation"] {{ background-color: #3b4252 !important; }} '
        + '.IZ65Hb-n0tgWb {{ border-color: rgba(136,192,208,0.22) !important; }}';
    }} else if (themeName === 'catppuccin') {{
      themeVars = 'body, html {{ background-color: #1e1e2e !important; }} '
        + 'header[role="banner"], .gb_Id, .gb_Kd, [role="navigation"] {{ background-color: #181825 !important; }} '
        + '.IZ65Hb-n0tgWb {{ border-color: rgba(249,226,175,0.18) !important; }}';
    }}

    var compactCss = compactMode
      ? 'header[role="banner"] a[aria-label*="Google Apps" i], header[role="banner"] a[aria-label*="Google apps" i], .gb_Se, .gb_Ve {{ display: none !important; }} '
        + 'header[role="banner"] {{ min-height: 52px !important; }} '
        + '@media (max-width: 780px) {{ [role="navigation"] {{ width: 64px !important; overflow: hidden !important; }} }}'
      : '';

    var styleText = ''
      + '::-webkit-scrollbar {{ width: 8px; height: 8px; }}'
      + '::-webkit-scrollbar-track {{ background: transparent; }}'
      + '::-webkit-scrollbar-thumb {{ background: rgba(154,160,166,0.32); border-radius: 999px; border: 2px solid transparent; background-clip: content-box; }}'
      + '::-webkit-scrollbar-thumb:hover {{ background: rgba(154,160,166,0.55); border: 2px solid transparent; background-clip: content-box; }}'
      + '* {{ scrollbar-width: thin; scrollbar-color: rgba(154,160,166,0.35) transparent; }}'
      + themeVars
      + compactCss;

    var styleEl = document.getElementById('kd-theme-style');
    if (!styleEl) {{
      styleEl = document.createElement('style');
      styleEl.id = 'kd-theme-style';
      (document.head || document.documentElement).appendChild(styleEl);
    }}
    styleEl.textContent = styleText;
  }} catch (_) {{}}
}})({theme_json}, {compact_mode});"#
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AppConfig {
    width: Option<u32>,
    height: Option<u32>,
    x: Option<i32>,
    y: Option<i32>,
    always_on_top: bool,
    zoom: f64,
    autostart: bool,
    sw_migrated_v2: bool,
    theme_preset: String,
    compact_mode: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            always_on_top: false,
            zoom: 1.0,
            autostart: false,
            sw_migrated_v2: false,
            theme_preset: "default".to_string(),
            compact_mode: false,
        }
    }
}

struct AppState {
    config: Mutex<AppConfig>,
    config_path: PathBuf,
    cached_labels: Mutex<Vec<String>>,
}

thread_local! {
    static KEEP_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    static KEEP_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
    static QUICK_NOTE_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    static QUICK_NOTE_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
}

fn quick_note_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL), Code::KeyK)
}

fn clamp_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return 1.0;
    }
    (zoom.clamp(MIN_ZOOM, MAX_ZOOM) * 100.0).round() / 100.0
}

fn format_zoom_label(zoom: f64) -> String {
    format!("Zoom atual: {}%", (zoom * 100.0).round() as i32)
}

fn load_config(path: &Path) -> AppConfig {
    let mut cfg = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<AppConfig>(&raw).ok())
        .unwrap_or_default();
    cfg.zoom = clamp_zoom(cfg.zoom);
    if !matches!(
        cfg.theme_preset.as_str(),
        "default" | "amoled" | "nord" | "catppuccin"
    ) {
        cfg.theme_preset = "default".to_string();
    }
    cfg
}

fn save_config(path: &Path, cfg: &AppConfig) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path, json);
    }
}

fn save_state_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(cfg) = state.config.lock() {
        save_config(&state.config_path, &cfg);
    };
}

fn with_keep_webview<F>(window: &Window, f: F)
where
    F: FnOnce(&wry::WebView) + Send + 'static,
{
    let _ = window.run_on_main_thread(move || {
        KEEP_WEBVIEW.with(|cell| {
            if let Some(wv) = cell.borrow().as_ref() {
                f(wv);
            }
        });
    });
}

fn with_quick_note_webview<F>(window: &Window, f: F)
where
    F: FnOnce(&wry::WebView) + Send + 'static,
{
    let _ = window.run_on_main_thread(move || {
        QUICK_NOTE_WEBVIEW.with(|cell| {
            if let Some(wv) = cell.borrow().as_ref() {
                f(wv);
            }
        });
    });
}

fn apply_main_ui_enhancements(app: &AppHandle) {
    let (theme, compact) = app
        .state::<AppState>()
        .config
        .lock()
        .map(|cfg| (cfg.theme_preset.clone(), cfg.compact_mode))
        .unwrap_or_else(|_| ("default".to_string(), false));

    if let Some(win) = app.get_window("main") {
        with_keep_webview(&win, move |wv| {
            let script = build_main_ui_js(&theme, compact);
            let _ = wv.evaluate_script(&script);
        });
    }
}

#[cfg(target_os = "windows")]
fn apply_quick_note_native_style(window: &Window) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DWM_WINDOW_CORNER_PREFERENCE,
        },
    };

    if let Ok(raw_hwnd) = window.hwnd() {
        let hwnd = HWND(raw_hwnd.0 as _);
        let pref = DWMWCP_ROUND;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_quick_note_native_style(_window: &Window) {}

fn show_and_focus_window(window: &Window) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    with_keep_webview(window, |wv| {
        let _ = wv.focus();
    });
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_visible && !is_minimized {
            save_state_config(app);
            let _ = window.hide();
        } else {
            show_and_focus_window(&window);
        }
    }
}

fn hide_quick_note(app: &AppHandle, clear: bool) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let _ = qn_win.hide();
        if clear {
            with_quick_note_webview(&qn_win, |wv| {
                let _ = wv.evaluate_script("window.resetQuickNote && window.resetQuickNote();");
            });
        }
    }
}

fn show_quick_note_error(app: &AppHandle, message: &str) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let msg_json =
            serde_json::to_string(message).unwrap_or_else(|_| "\"Erro ao salvar\"".to_string());
        let _ = qn_win.show();
        let _ = qn_win.unminimize();
        let _ = qn_win.set_focus();
        with_quick_note_webview(&qn_win, move |wv| {
            let _ = wv.focus();
            let js =
                format!("window.onQuickNoteSaveError && window.onQuickNoteSaveError({msg_json});");
            let _ = wv.evaluate_script(&js);
        });
    }
}

fn push_labels_to_quick_note(app: &AppHandle, labels: &[String]) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let json = serde_json::to_string(labels).unwrap_or_else(|_| "[]".to_string());
        with_quick_note_webview(&qn_win, move |wv| {
            let js = format!("window.setAvailableLabels && window.setAvailableLabels({json});");
            let _ = wv.evaluate_script(&js);
        });
    }
}

#[derive(Deserialize)]
struct KeepLabelsResult {
    ready: bool,
    labels: Vec<String>,
}

fn parse_eval_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Option<T> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return None;
    }
    serde_json::from_str::<T>(trimmed).ok().or_else(|| {
        serde_json::from_str::<String>(trimmed)
            .ok()
            .and_then(|inner| serde_json::from_str::<T>(&inner).ok())
    })
}

fn sync_keep_labels(app: &AppHandle) {
    if let Some(main_win) = app.get_window("main") {
        let app_handle = app.clone();
        with_keep_webview(&main_win, move |wv| {
            if let Ok(url) = wv.url() {
                if !url.contains("keep.google.com") || url.contains("accounts.google.com") {
                    return;
                }
            }
            let cb_app = app_handle.clone();
            let _ = wv.evaluate_script_with_callback(EXTRACT_KEEP_LABELS_JS, move |raw| {
                let parsed = if let Some(res) = parse_eval_json::<KeepLabelsResult>(&raw) {
                    if !res.ready {
                        return;
                    }
                    res.labels
                } else if let Some(list) = parse_eval_json::<Vec<String>>(&raw) {
                    list
                } else {
                    return;
                };
                let target_app = cb_app.clone();
                let _ = cb_app.run_on_main_thread(move || {
                    let state = target_app.state::<AppState>();
                    let latest = if let Ok(mut cached) = state.cached_labels.lock() {
                        // Preserva labels recém-criados se o Keep ainda estiver terminando de renderizar
                        if !parsed.is_empty() || cached.is_empty() {
                            *cached = parsed;
                        }
                        cached.clone()
                    } else {
                        parsed
                    };
                    push_labels_to_quick_note(&target_app, &latest);
                });
            });
        });
    }
}

fn trigger_quick_note(app: &AppHandle) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let is_visible = qn_win.is_visible().unwrap_or(false);
        let is_focused = qn_win.is_focused().unwrap_or(false);
        if is_visible && is_focused {
            hide_quick_note(app, true);
            return;
        }
        let cached = app
            .state::<AppState>()
            .cached_labels
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default();
        push_labels_to_quick_note(app, &cached);
        sync_keep_labels(app);

        let _ = qn_win.center();
        let _ = qn_win.show();
        let _ = qn_win.unminimize();
        let _ = qn_win.set_focus();
        with_quick_note_webview(&qn_win, |wv| {
            let _ = wv.focus();
            let _ = wv.evaluate_script("window.focusQuickNote && window.focusQuickNote();");
        });
    }
}

#[derive(Deserialize)]
struct SubmitStatusResult {
    id: u64,
    done: bool,
    ok: bool,
    #[serde(default)]
    stage: String,
    #[serde(default)]
    detail: String,
}

fn submit_quick_note_to_keep(
    app: &AppHandle,
    title: String,
    body: String,
    is_checklist: bool,
    items: Vec<ChecklistItemInput>,
    color_labels: Vec<String>,
    pinned: bool,
    tags: Vec<String>,
) {
    let has_items = is_checklist && items.iter().any(|i| !i.text.trim().is_empty());
    if title.trim().is_empty() && body.trim().is_empty() && !has_items {
        return;
    }
    if !tags.is_empty() {
        let state = app.state::<AppState>();
        if let Ok(mut cached) = state.cached_labels.lock() {
            for t in &tags {
                let clean = t.trim();
                if !clean.is_empty()
                    && !cached
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(clean))
                {
                    cached.push(clean.to_string());
                }
            }
            cached.sort_by_key(|a| a.to_lowercase());
        };
    }

    let submit_id = NEXT_SUBMIT_ID.fetch_add(1, Ordering::Relaxed);
    hide_quick_note(app, false);

    if let Some(main_win) = app.get_window("main") {
        let main_win_clone = main_win.clone();
        let app_for_auth = app.clone();
        let poll_app = app.clone();
        with_keep_webview(&main_win, move |wv| {
            if let Ok(url) = wv.url() {
                if url.contains("accounts.google.com") || !url.contains("keep.google.com") {
                    show_and_focus_window(&main_win_clone);
                    show_quick_note_error(
                        &app_for_auth,
                        "Faça login no Google Keep e tente novamente",
                    );
                    return;
                }
            }
            let script = build_submit_note_js(
                submit_id,
                &title,
                &body,
                is_checklist,
                &items,
                &color_labels,
                pinned,
                &tags,
            );
            let _ = wv.evaluate_script(&script);

            std::thread::spawn(move || {
                for _ in 0..40 {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let (tx, rx) = std::sync::mpsc::channel::<Option<SubmitStatusResult>>();
                    let check_app = poll_app.clone();
                    let sent = poll_app.run_on_main_thread(move || {
                        if let Some(win) = check_app.get_window("main") {
                            KEEP_WEBVIEW.with(|cell| {
                                if let Some(wv) = cell.borrow().as_ref() {
                                    let _ = wv.evaluate_script_with_callback(
                                        "window.__kdSubmitStatus || null",
                                        move |raw| {
                                            let _ = tx
                                                .send(parse_eval_json::<SubmitStatusResult>(&raw));
                                        },
                                    );
                                } else {
                                    let _ = tx.send(None);
                                }
                            });
                            let _ = &win;
                        } else {
                            let _ = tx.send(None);
                        }
                    });
                    if sent.is_err() {
                        break;
                    }
                    if let Ok(Some(status)) = rx.recv_timeout(std::time::Duration::from_millis(800))
                    {
                        if status.id == submit_id && status.done {
                            let finish_app = poll_app.clone();
                            let _ = poll_app.run_on_main_thread(move || {
                                if status.ok {
                                    if let Some(qn_win) = finish_app.get_window("quick-note") {
                                        with_quick_note_webview(&qn_win, |wv| {
                                            let _ = wv.evaluate_script(
                                                "window.resetQuickNoteIfSaving && window.resetQuickNoteIfSaving();",
                                            );
                                        });
                                    }
                                    sync_keep_labels(&finish_app);
                                } else {
                                    let msg = if status.detail.is_empty() {
                                        format!("Erro ao salvar no Keep ({})", status.stage)
                                    } else {
                                        format!("Erro ({}): {}", status.stage, status.detail)
                                    };
                                    show_quick_note_error(&finish_app, &msg);
                                }
                            });
                            return;
                        }
                    }
                }
                let timeout_app = poll_app.clone();
                let _ = poll_app.run_on_main_thread(move || {
                    show_quick_note_error(
                        &timeout_app,
                        "Tempo esgotado ao salvar — verifique se o Keep está logado",
                    );
                });
            });
        });
    } else {
        show_quick_note_error(app, "Janela principal do Keep indisponível");
    }
}

fn apply_zoom(app: &AppHandle, zoom_status_item: &MenuItem<tauri::Wry>, next_zoom: f64) {
    let clamped = clamp_zoom(next_zoom);
    let state = app.state::<AppState>();
    if let Ok(mut cfg) = state.config.lock() {
        cfg.zoom = clamped;
        save_config(&state.config_path, &cfg);
    }
    let _ = zoom_status_item.set_text(format_zoom_label(clamped));
    if let Some(win) = app.get_window("main") {
        with_keep_webview(&win, move |wv| {
            let _ = wv.zoom(clamped);
        });
    }
}

fn is_position_on_any_monitor(window: &Window, x: i32, y: i32, width: u32, height: u32) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return x > -10000 && y > -10000;
    };
    if monitors.is_empty() {
        return x > -10000 && y > -10000;
    }
    let w = width as i32;
    let h = height as i32;
    monitors.into_iter().any(|m| {
        let mp = m.position();
        let ms = m.size();
        let mw = ms.width as i32;
        let mh = ms.height as i32;
        let overlap_x = (x + w).min(mp.x + mw) - x.max(mp.x);
        let overlap_y = (y + h).min(mp.y + mh) - y.max(mp.y);
        overlap_x >= 80 && overlap_y >= 60
    })
}

fn resolve_target_url(raw: &str) -> String {
    if let Ok(parsed) = Url::parse(raw) {
        if parsed.host_str() == Some("www.google.com") && parsed.path() == "/url" {
            for (k, v) in parsed.query_pairs() {
                if (k == "q" || k == "url")
                    && (v.starts_with("https://") || v.starts_with("http://"))
                {
                    return v.into_owned();
                }
            }
        }
    }
    raw.to_string()
}

fn should_open_in_app(raw: &str) -> bool {
    let Ok(parsed) = Url::parse(raw) else {
        return true;
    };
    match parsed.scheme() {
        "about" | "blob" | "javascript" | "data" => true,
        "http" | "https" => matches!(
            parsed.host_str(),
            Some(
                "keep.google.com"
                    | "accounts.google.com"
                    | "accounts.youtube.com"
                    | "gds.google.com"
                    | "ogs.google.com"
            )
        ),
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn open_external_url(url: &str) {
    use windows::{
        core::{HSTRING, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };

    let verb = HSTRING::from("open");
    let target = HSTRING::from(url);
    let _ = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
}

#[cfg(not(target_os = "windows"))]
fn open_external_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(target_os = "windows")]
fn is_autostart_enabled() -> bool {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(AUTOSTART_REG_KEY, KEY_READ) else {
        return false;
    };
    key.get_value::<String, _>(AUTOSTART_VALUE_NAME).is_ok()
}

#[cfg(target_os = "linux")]
fn linux_autostart_file() -> Option<PathBuf> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(
        config_dir
            .join("autostart")
            .join("com.kyouztech.keepdesktop.desktop"),
    )
}

#[cfg(target_os = "linux")]
fn is_autostart_enabled() -> bool {
    linux_autostart_file().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn is_autostart_enabled() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --minimized", exe.display());
        let (key, _) = hkcu
            .create_subkey(AUTOSTART_REG_KEY)
            .map_err(|e| e.to_string())?;
        key.set_value(AUTOSTART_VALUE_NAME, &command)
            .map_err(|e| e.to_string())?;
    } else if let Ok(key) = hkcu.open_subkey_with_flags(AUTOSTART_REG_KEY, KEY_WRITE) {
        if let Err(err) = key.delete_value(AUTOSTART_VALUE_NAME) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = linux_autostart_file()
        .ok_or_else(|| "Diretório de configuração XDG não encontrado".to_string())?;
    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let exec_line = if std::env::var_os("FLATPAK_ID").is_some() {
            "flatpak run com.kyouztech.keepdesktop --minimized".to_string()
        } else {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            format!("\"{}\" --minimized", exe.display())
        };
        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Google Keep\n\
             Comment=Google Keep Desktop\n\
             Exec={exec_line}\n\
             Icon=com.kyouztech.keepdesktop\n\
             Terminal=false\n\
             Categories=Office;Utility;\n\
             X-GNOME-Autostart-enabled=true\n"
        );
        fs::write(&path, entry).map_err(|e| e.to_string())?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn set_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_windows_profile_and_navigate(
    webview: &wry::WebView,
    window: &Window,
    clear_cache_storage: bool,
) {
    use webview2_com::{
        ClearBrowsingDataCompletedHandler,
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Profile2, ICoreWebView2Profile3, ICoreWebView2_13,
            COREWEBVIEW2_BROWSING_DATA_KINDS, COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
            COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS,
            COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
        },
    };
    use windows_core::Interface;
    use wry::WebViewExtWindows;

    let core = webview.webview();
    unsafe {
        if let Ok(webview_13) = core.cast::<ICoreWebView2_13>() {
            if let Ok(profile) = webview_13.Profile() {
                if let Ok(profile_3) = profile.cast::<ICoreWebView2Profile3>() {
                    let _ = profile_3.SetPreferredTrackingPreventionLevel(
                        COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
                    );
                }

                if let Ok(profile_2) = profile.cast::<ICoreWebView2Profile2>() {
                    // Limpa Service Workers (e CacheStorage na migração) sem apagar cookies ou sessão Google.
                    let mut flags = COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS.0;
                    if clear_cache_storage {
                        flags |= COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0;
                    }
                    let data_kinds = COREWEBVIEW2_BROWSING_DATA_KINDS(flags);
                    let nav_window = window.clone();
                    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |_| {
                        with_keep_webview(&nav_window, |wv| {
                            let _ = wv.load_url(KEEP_URL);
                        });
                        Ok(())
                    }));
                    if profile_2.ClearBrowsingData(data_kinds, &handler).is_ok() {
                        return;
                    }
                }
            }
        }
    }

    let _ = webview.load_url(KEEP_URL);
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_profile_and_navigate(
    webview: &wry::WebView,
    _window: &Window,
    _clear_cache_storage: bool,
) {
    let _ = webview.load_url(KEEP_URL);
}

#[cfg(target_os = "windows")]
fn register_zoom_listener(
    webview: &wry::WebView,
    app: &AppHandle,
    zoom_status_item: MenuItem<tauri::Wry>,
) {
    use webview2_com::ZoomFactorChangedEventHandler;
    use wry::WebViewExtWindows;

    let app_handle = app.clone();
    let handler = ZoomFactorChangedEventHandler::create(Box::new(move |controller, _| {
        if let Some(ctrl) = controller {
            let mut factor = 1.0_f64;
            if unsafe { ctrl.ZoomFactor(&mut factor) }.is_ok() {
                let rounded = clamp_zoom(factor);
                let state = app_handle.state::<AppState>();
                let changed = if let Ok(mut cfg) = state.config.lock() {
                    if (cfg.zoom - rounded).abs() >= 0.005 {
                        cfg.zoom = rounded;
                        save_config(&state.config_path, &cfg);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if changed {
                    let _ = zoom_status_item.set_text(format_zoom_label(rounded));
                }
            }
        }
        Ok(())
    }));

    let mut token: i64 = 0;
    unsafe {
        let _ = webview
            .controller()
            .add_ZoomFactorChanged(&handler, &mut token);
    }
}

#[cfg(target_os = "linux")]
fn register_zoom_listener(
    webview: &wry::WebView,
    app: &AppHandle,
    zoom_status_item: MenuItem<tauri::Wry>,
) {
    use gtk::prelude::WidgetExt;
    use wry::WebViewExtUnix;

    let gtk_wv = webview.webview();

    let app_key = app.clone();
    let zoom_item_key = zoom_status_item.clone();
    gtk_wv.connect_key_press_event(move |_, event| {
        if !event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let kv = event.keyval();
        let current = app_key
            .state::<AppState>()
            .config
            .lock()
            .map(|cfg| cfg.zoom)
            .unwrap_or(1.0);
        if kv == gdk::keys::constants::plus
            || kv == gdk::keys::constants::equal
            || kv == gdk::keys::constants::KP_Add
            || kv == gdk::keys::constants::KP_Equal
        {
            apply_zoom(&app_key, &zoom_item_key, current + ZOOM_STEP);
            return glib::Propagation::Stop;
        }
        if kv == gdk::keys::constants::minus || kv == gdk::keys::constants::KP_Subtract {
            apply_zoom(&app_key, &zoom_item_key, current - ZOOM_STEP);
            return glib::Propagation::Stop;
        }
        if kv == gdk::keys::constants::_0 || kv == gdk::keys::constants::KP_0 {
            apply_zoom(&app_key, &zoom_item_key, 1.0);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    let app_scroll = app.clone();
    let zoom_item_scroll = zoom_status_item;
    gtk_wv.connect_scroll_event(move |_, event| {
        if !event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let current = app_scroll
            .state::<AppState>()
            .config
            .lock()
            .map(|cfg| cfg.zoom)
            .unwrap_or(1.0);
        match event.direction() {
            gdk::ScrollDirection::Up => {
                apply_zoom(&app_scroll, &zoom_item_scroll, current + ZOOM_STEP);
                glib::Propagation::Stop
            }
            gdk::ScrollDirection::Down => {
                apply_zoom(&app_scroll, &zoom_item_scroll, current - ZOOM_STEP);
                glib::Propagation::Stop
            }
            gdk::ScrollDirection::Smooth => {
                let (_, dy) = event.delta();
                if dy < -0.05 {
                    apply_zoom(&app_scroll, &zoom_item_scroll, current + ZOOM_STEP);
                    glib::Propagation::Stop
                } else if dy > 0.05 {
                    apply_zoom(&app_scroll, &zoom_item_scroll, current - ZOOM_STEP);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
            _ => glib::Propagation::Proceed,
        }
    });
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn register_zoom_listener(
    _webview: &wry::WebView,
    _app: &AppHandle,
    _zoom_status_item: MenuItem<tauri::Wry>,
) {
}

fn main() {
    let start_minimized = std::env::args().any(|arg| arg == "--minimized");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let minimized = args.iter().any(|arg| arg == "--minimized");
            if !minimized {
                let app_handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(win) = app_handle.get_window("main") {
                        show_and_focus_window(&win);
                    }
                });
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &quick_note_shortcut() {
                        trigger_quick_note(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let data_dir = app.path().app_local_data_dir().ok();
            let qn_data_dir = data_dir.clone().map(|d| d.join("quick-note"));
            let config_path = data_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir)
                .join("desktop-config.json");

            let mut initial_cfg = load_config(&config_path);
            let system_autostart = is_autostart_enabled();
            if initial_cfg.autostart != system_autostart {
                initial_cfg.autostart = system_autostart;
            }
            // Garante que o comando salvo reflita a versão atual caso habilitado
            if initial_cfg.autostart {
                let _ = set_autostart(true);
            }

            let clear_cache_storage = !initial_cfg.sw_migrated_v2;
            if clear_cache_storage {
                initial_cfg.sw_migrated_v2 = true;
            }
            save_config(&config_path, &initial_cfg);

            let initial_zoom = initial_cfg.zoom;
            let initial_pin = initial_cfg.always_on_top;
            let initial_autostart = initial_cfg.autostart;
            let initial_theme = initial_cfg.theme_preset.clone();
            let initial_compact = initial_cfg.compact_mode;
            let saved_size = initial_cfg.width.zip(initial_cfg.height);
            let saved_pos = initial_cfg.x.zip(initial_cfg.y);

            app.manage(AppState {
                config: Mutex::new(initial_cfg),
                config_path,
                cached_labels: Mutex::new(Vec::new()),
            });

            // Registra Ctrl+K exclusivamente para abrir a janela rápida de nova nota
            if let Err(err) = app.global_shortcut().register(quick_note_shortcut()) {
                eprintln!("Aviso: não foi possível registrar Ctrl+K: {err}");
            }

            // Cria janela nativa Tauri sem WebView Tauri (evita injeção de window.ipc / __TAURI_INTERNALS__
            // e filtros WebResourceRequested que quebram o bundle Closure do Google Keep)
            let mut window_builder = Window::builder(app, "main")
                .title("Google Keep")
                .inner_size(1100.0, 760.0)
                .resizable(true)
                .center()
                .visible(false)
                .theme(Some(Theme::Dark));

            if let Some(icon) = app.default_window_icon() {
                window_builder = window_builder.icon(icon.clone())?;
            }

            let window = window_builder.build()?;

            let (restore_w, restore_h) = match saved_size {
                Some((w, h)) if w >= 320 && h >= 240 => {
                    let _ = window.set_size(PhysicalSize::new(w, h));
                    (w, h)
                }
                _ => {
                    let s = window
                        .inner_size()
                        .unwrap_or_else(|_| PhysicalSize::new(1100, 760));
                    (s.width, s.height)
                }
            };

            if let Some((x, y)) = saved_pos {
                if is_position_on_any_monitor(&window, x, y, restore_w, restore_h) {
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                }
            }

            if initial_pin {
                let _ = window.set_always_on_top(true);
            }

            if !start_minimized {
                let _ = window.show();
                let _ = window.set_focus();
            }

            let zoom_status_item = MenuItem::with_id(
                app,
                "zoom_status",
                format_zoom_label(initial_zoom),
                false,
                None::<&str>,
            )?;

            let mut web_context = wry::WebContext::new(data_dir);
            let page_load_app = app.handle().clone();

            let webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_devtools(true)
                    .with_clipboard(true)
                    .with_hotkeys_zoom(true)
                    .with_new_window_req_handler(|url, _| {
                        let resolved = resolve_target_url(&url);
                        if should_open_in_app(&resolved) {
                            wry::NewWindowResponse::Allow
                        } else {
                            open_external_url(&resolved);
                            wry::NewWindowResponse::Deny
                        }
                    })
                    .with_on_page_load_handler(move |event, _| {
                        if matches!(event, PageLoadEvent::Finished) {
                            let state = page_load_app.state::<AppState>();
                            let (zoom, theme, compact) = state
                                .config
                                .lock()
                                .map(|cfg| (cfg.zoom, cfg.theme_preset.clone(), cfg.compact_mode))
                                .unwrap_or_else(|_| (1.0, "default".to_string(), false));
                            if let Some(win) = page_load_app.get_window("main") {
                                with_keep_webview(&win, move |wv| {
                                    let _ = wv.zoom(zoom);
                                    let script = build_main_ui_js(&theme, compact);
                                    let _ = wv.evaluate_script(&script);
                                });
                            }
                            sync_keep_labels(&page_load_app);
                            let delayed_app = page_load_app.clone();
                            std::thread::spawn(move || {
                                for delay_ms in [1500_u64, 2500_u64] {
                                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                                    let app_inner = delayed_app.clone();
                                    let _ = delayed_app.run_on_main_thread(move || {
                                        sync_keep_labels(&app_inner);
                                    });
                                }
                            });
                        }
                    });

                #[cfg(target_os = "windows")]
                {
                    use wry::WebViewBuilderExtWindows;
                    builder = builder
                        .with_theme(wry::Theme::Dark)
                        .with_additional_browser_args(
                            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,ServiceWorkerStaticRouter,ThirdPartyStoragePartitioning,TrackingProtection3pcd --disable-background-timer-throttling --disable-backgrounding-occluded-windows --disable-renderer-backgrounding",
                        )
                        .with_url("about:blank");
                }

                #[cfg(not(target_os = "windows"))]
                {
                    builder = builder.with_url(KEEP_URL);
                }

                #[cfg(target_os = "linux")]
                {
                    use wry::WebViewBuilderExtUnix;
                    let vbox = window.default_vbox()?;
                    let wv = builder.build_gtk(&vbox)?;
                    let _ = wv.set_visible(true);
                    wv
                }

                #[cfg(not(target_os = "linux"))]
                {
                    builder.build(&window)?
                }
            };

            let _ = webview.zoom(initial_zoom);
            register_zoom_listener(&webview, app.handle(), zoom_status_item.clone());

            KEEP_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(web_context);
            });
            KEEP_WEBVIEW.with(|cell| {
                if cfg!(target_os = "windows") {
                    configure_windows_profile_and_navigate(&webview, &window, clear_cache_storage);
                }
                *cell.borrow_mut() = Some(webview);
            });

            // Mini-janela dedicada para anotação rápida (Ctrl+K) sem exibir a janela principal
            let mut qn_builder = Window::builder(app, "quick-note")
                .title("Nova nota rápida — Google Keep")
                .inner_size(480.0, 330.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .center()
                .visible(false)
                .theme(Some(Theme::Dark));

            if let Some(icon) = app.default_window_icon() {
                qn_builder = qn_builder.icon(icon.clone())?;
            }

            let qn_window = qn_builder.build()?;
            apply_quick_note_native_style(&qn_window);

            let mut qn_web_context = wry::WebContext::new(qn_data_dir);
            let ipc_app = app.handle().clone();
            let qn_webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut qn_web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_clipboard(true)
                    .with_html(QUICK_NOTE_HTML)
                    .with_ipc_handler(move |req| {
                        if let Ok(msg) = serde_json::from_str::<QuickNoteIpc>(req.body()) {
                            let app_handle = ipc_app.clone();
                            let _ = ipc_app.run_on_main_thread(move || match msg {
                                QuickNoteIpc::Submit {
                                    title,
                                    body,
                                    is_checklist,
                                    items,
                                    color_labels,
                                    pinned,
                                    tags,
                                } => {
                                    submit_quick_note_to_keep(
                                        &app_handle,
                                        title,
                                        body,
                                        is_checklist,
                                        items,
                                        color_labels,
                                        pinned,
                                        tags,
                                    );
                                }
                                QuickNoteIpc::RefreshLabels => {
                                    sync_keep_labels(&app_handle);
                                }
                                QuickNoteIpc::Close => {
                                    hide_quick_note(&app_handle, true);
                                }
                                QuickNoteIpc::OpenMain => {
                                    hide_quick_note(&app_handle, false);
                                    if let Some(main_win) = app_handle.get_window("main") {
                                        show_and_focus_window(&main_win);
                                    }
                                }
                                QuickNoteIpc::Drag => {
                                    if let Some(qn_win) = app_handle.get_window("quick-note") {
                                        let _ = qn_win.start_dragging();
                                    }
                                }
                                QuickNoteIpc::Center => {
                                    if let Some(qn_win) = app_handle.get_window("quick-note") {
                                        let _ = qn_win.center();
                                    }
                                }
                                QuickNoteIpc::Resize { height } => {
                                    if let Some(qn_win) = app_handle.get_window("quick-note") {
                                        let h = height.clamp(320.0, 560.0);
                                        let _ =
                                            qn_win.set_size(tauri::LogicalSize::new(480.0, h));
                                    }
                                }
                            });
                        }
                    });

                #[cfg(target_os = "windows")]
                {
                    use wry::WebViewBuilderExtWindows;
                    builder = builder.with_theme(wry::Theme::Dark);
                }

                #[cfg(target_os = "linux")]
                {
                    use wry::WebViewBuilderExtUnix;
                    let vbox = qn_window.default_vbox()?;
                    let wv = builder.build_gtk(&vbox)?;
                    let _ = wv.set_visible(true);
                    wv
                }

                #[cfg(not(target_os = "linux"))]
                {
                    builder.build(&qn_window)?
                }
            };

            QUICK_NOTE_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(qn_web_context);
            });
            QUICK_NOTE_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(qn_webview);
            });

            // Cria menu da bandeja do sistema (System Tray)
            let open_item =
                MenuItem::with_id(app, "open", "Abrir / Ocultar Keep", true, None::<&str>)?;
            let quick_note_item = MenuItem::with_id(
                app,
                "quick_note",
                "Nova nota rápida (Ctrl+K)",
                true,
                None::<&str>,
            )?;
            let reload_item =
                MenuItem::with_id(app, "reload", "Recarregar página", true, None::<&str>)?;
            let sep_top = PredefinedMenuItem::separator(app)?;
            let pin_item = CheckMenuItem::with_id(
                app,
                "pin",
                "Sempre no topo (Always on Top)",
                true,
                initial_pin,
                None::<&str>,
            )?;
            let autostart_item = CheckMenuItem::with_id(
                app,
                "autostart",
                AUTOSTART_MENU_LABEL,
                true,
                initial_autostart,
                None::<&str>,
            )?;

            let theme_default_item = CheckMenuItem::with_id(
                app,
                "theme_default",
                "Tema: Padrão Keep",
                true,
                initial_theme == "default",
                None::<&str>,
            )?;
            let theme_amoled_item = CheckMenuItem::with_id(
                app,
                "theme_amoled",
                "Tema: AMOLED (Preto puro)",
                true,
                initial_theme == "amoled",
                None::<&str>,
            )?;
            let theme_nord_item = CheckMenuItem::with_id(
                app,
                "theme_nord",
                "Tema: Nord Escuro",
                true,
                initial_theme == "nord",
                None::<&str>,
            )?;
            let theme_catppuccin_item = CheckMenuItem::with_id(
                app,
                "theme_catppuccin",
                "Tema: Catppuccin Mocha",
                true,
                initial_theme == "catppuccin",
                None::<&str>,
            )?;
            let sep_appearance = PredefinedMenuItem::separator(app)?;
            let compact_mode_item = CheckMenuItem::with_id(
                app,
                "compact_mode",
                "Modo Compacto / Foco",
                true,
                initial_compact,
                None::<&str>,
            )?;
            let appearance_submenu = Submenu::with_id_and_items(
                app,
                "appearance_menu",
                "Aparência",
                true,
                &[
                    &theme_default_item,
                    &theme_amoled_item,
                    &theme_nord_item,
                    &theme_catppuccin_item,
                    &sep_appearance,
                    &compact_mode_item,
                ],
            )?;

            let zoom_in_item =
                MenuItem::with_id(app, "zoom_in", "Aumentar (+10%)", true, None::<&str>)?;
            let zoom_out_item =
                MenuItem::with_id(app, "zoom_out", "Diminuir (-10%)", true, None::<&str>)?;
            let zoom_reset_item =
                MenuItem::with_id(app, "zoom_reset", "Restaurar (100%)", true, None::<&str>)?;
            let zoom_submenu = Submenu::with_id_and_items(
                app,
                "zoom_menu",
                "Zoom",
                true,
                &[
                    &zoom_status_item,
                    &zoom_in_item,
                    &zoom_out_item,
                    &zoom_reset_item,
                ],
            )?;

            let sep_mid = PredefinedMenuItem::separator(app)?;
            let devtools_item =
                MenuItem::with_id(app, "devtools", "Abrir DevTools (F12)", true, None::<&str>)?;
            let sep_bottom = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &open_item,
                    &quick_note_item,
                    &reload_item,
                    &sep_top,
                    &pin_item,
                    &autostart_item,
                    &appearance_submenu,
                    &zoom_submenu,
                    &sep_mid,
                    &devtools_item,
                    &sep_bottom,
                    &quit_item,
                ],
            )?;

            let pin_clone = pin_item.clone();
            let autostart_clone = autostart_item.clone();
            let zoom_status_clone = zoom_status_item.clone();
            let theme_default_clone = theme_default_item.clone();
            let theme_amoled_clone = theme_amoled_item.clone();
            let theme_nord_clone = theme_nord_item.clone();
            let theme_catppuccin_clone = theme_catppuccin_item.clone();
            let compact_clone = compact_mode_item.clone();

            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "open" => toggle_window(app),
                    "quick_note" => trigger_quick_note(app),
                    "reload" => {
                        if let Some(win) = app.get_window("main") {
                            with_keep_webview(&win, |wv| {
                                let _ = wv.reload();
                            });
                        }
                    }
                    "pin" => {
                        let state = app.state::<AppState>();
                        let next = if let Ok(mut cfg) = state.config.lock() {
                            cfg.always_on_top = !cfg.always_on_top;
                            save_config(&state.config_path, &cfg);
                            cfg.always_on_top
                        } else {
                            false
                        };
                        let _ = pin_clone.set_checked(next);
                        if let Some(win) = app.get_window("main") {
                            let _ = win.set_always_on_top(next);
                        }
                    }
                    "autostart" => {
                        let state = app.state::<AppState>();
                        let desired = state
                            .config
                            .lock()
                            .map(|cfg| !cfg.autostart)
                            .unwrap_or(false);
                        match set_autostart(desired) {
                            Ok(()) => {
                                if let Ok(mut cfg) = state.config.lock() {
                                    cfg.autostart = desired;
                                    save_config(&state.config_path, &cfg);
                                }
                                let _ = autostart_clone.set_checked(desired);
                            }
                            Err(err) => {
                                eprintln!("Erro ao atualizar autostart: {err}");
                                let current = is_autostart_enabled();
                                let _ = autostart_clone.set_checked(current);
                            }
                        }
                    }
                    "theme_default" | "theme_amoled" | "theme_nord" | "theme_catppuccin" => {
                        let chosen = match event.id.as_ref() {
                            "theme_amoled" => "amoled",
                            "theme_nord" => "nord",
                            "theme_catppuccin" => "catppuccin",
                            _ => "default",
                        };
                        let state = app.state::<AppState>();
                        if let Ok(mut cfg) = state.config.lock() {
                            cfg.theme_preset = chosen.to_string();
                            save_config(&state.config_path, &cfg);
                        }
                        let _ = theme_default_clone.set_checked(chosen == "default");
                        let _ = theme_amoled_clone.set_checked(chosen == "amoled");
                        let _ = theme_nord_clone.set_checked(chosen == "nord");
                        let _ = theme_catppuccin_clone.set_checked(chosen == "catppuccin");
                        apply_main_ui_enhancements(app);
                    }
                    "compact_mode" => {
                        let state = app.state::<AppState>();
                        let next = if let Ok(mut cfg) = state.config.lock() {
                            cfg.compact_mode = !cfg.compact_mode;
                            save_config(&state.config_path, &cfg);
                            cfg.compact_mode
                        } else {
                            false
                        };
                        let _ = compact_clone.set_checked(next);
                        apply_main_ui_enhancements(app);
                    }
                    "zoom_in" => {
                        let current = app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|cfg| cfg.zoom)
                            .unwrap_or(1.0);
                        apply_zoom(app, &zoom_status_clone, current + ZOOM_STEP);
                    }
                    "zoom_out" => {
                        let current = app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|cfg| cfg.zoom)
                            .unwrap_or(1.0);
                        apply_zoom(app, &zoom_status_clone, current - ZOOM_STEP);
                    }
                    "zoom_reset" => {
                        apply_zoom(app, &zoom_status_clone, 1.0);
                    }
                    "devtools" => {
                        if let Some(win) = app.get_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            with_keep_webview(&win, |wv| {
                                wv.open_devtools();
                            });
                        }
                    }
                    "quit" => {
                        save_state_config(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            match tray_builder.build(app) {
                Ok(_) => TRAY_AVAILABLE.store(true, Ordering::Relaxed),
                Err(err) => {
                    eprintln!("Aviso: bandeja do sistema indisponível: {err}");
                    let _ = window.show();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "quick-note" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    hide_quick_note(window.app_handle(), true);
                    api.prevent_close();
                }
                return;
            }
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::Resized(size) => {
                    if !window.is_minimized().unwrap_or(false)
                        && window.is_visible().unwrap_or(false)
                        && size.width >= 320
                        && size.height >= 240
                    {
                        let state = window.app_handle().state::<AppState>();
                        if let Ok(mut cfg) = state.config.lock() {
                            cfg.width = Some(size.width);
                            cfg.height = Some(size.height);
                        };
                    }
                }
                WindowEvent::Moved(pos) => {
                    if !window.is_minimized().unwrap_or(false)
                        && window.is_visible().unwrap_or(false)
                        && pos.x > -10000
                        && pos.y > -10000
                    {
                        let state = window.app_handle().state::<AppState>();
                        if let Ok(mut cfg) = state.config.lock() {
                            cfg.x = Some(pos.x);
                            cfg.y = Some(pos.y);
                        };
                    }
                }
                WindowEvent::Focused(false) => {
                    save_state_config(window.app_handle());
                }
                // Oculta na bandeja quando disponível; caso contrário fecha normalmente
                WindowEvent::CloseRequested { api, .. } => {
                    save_state_config(window.app_handle());
                    if TRAY_AVAILABLE.load(Ordering::Relaxed) {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("Erro ao executar aplicação Tauri");
}
