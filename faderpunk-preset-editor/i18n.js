/**
 * Faderpunk Preset Editor — EN / DE / FR
 * Usage: t("key"), t("key", { name: "x" }), applyI18n(), setLang("en"|"de"|"fr")
 */
(function (global) {
  const LANG_KEY = "fp-lang-v1";
  const SUPPORTED = ["en", "de", "fr"];

  const I18N = {
    en: {
      "doc.title": "Faderpunk Preset Editor",
      "app.h1": "Preset Editor",
      "app.compat":
        "Works with the <a href=\"https://faderpunk.io/beta/#/configurator\" target=\"_blank\" rel=\"noopener noreferrer\">faderpunk.io/beta</a> configurator",
      "lang.label": "Language",

      "push.fabTitle": "Push the active preset to the Faderpunk",
      "push.fabLine1": "Push",
      "push.fabLine2": "to Punk",

      "inst.summary": "Your MIDI instruments",
      "inst.groupNew": "New",
      "inst.groupList": "Library",
      "inst.empty": "No instruments yet.",
      "inst.remove": "Remove",
      "inst.instrument": "Instrument",
      "inst.name": "Name",
      "inst.midiCh": "CH",
      "inst.csv": "CC CSV",
      "inst.recent": "Recently used",
      "inst.all": "All instruments",
      "inst.csvNone": "choose (or not)",
      "inst.csvEmpty": "— no CSV —",
      "inst.manual": "— manual —",
      "inst.add": "Add instrument",
      "inst.csvUpload": "Upload CSV",
      "inst.csvUploadTitle":
        "Add a CC/NRPN CSV to midi-custom/ (shows up as Custom/… in the list)",
      "inst.csvUploadOk": "CSV uploaded: {path}",
      "inst.csvUploadFail": "CSV upload failed: {msg}",
      "inst.midiSync": "Fetch CCs online",
      "inst.midiSyncTitle":
        "Download the MIDI CC/NRPN CSV library from midi.guide (pencilresearch/midi on GitHub). Your uploaded midi-custom/ files stay.",
      "inst.midiSyncOk": "Online CCs: {message}",
      "inst.midiSyncFail": "Online CC fetch failed: {msg}",
      "inst.removeConfirm": 'Remove instrument "{name}"?',
      "inst.chUpdated":
        'Instrument "{name}" → CH{ch} · {n} row(s) updated',

      "tabs.aria": "Presets",
      "tabs.remove": "Remove preset",
      "tabs.add": "Add preset",
      "tabs.removeAria": "Remove",
      "tabs.addAria": "Add",
      "tabs.dup": "Duplicate preset",
      "tabs.dupAria": "Duplicate",
      "tabs.io": "Export / Import",

      "bar.shortName": "Short name",
      "bar.desc": "Description",
      "bar.defaultPort": "Default port",
      "bar.defaultPortTitle":
        'Applies to rows with port "default". Rows with their own port ignore this.',
      "bar.portAll": "Apply to all",
      "bar.portAllTitle":
        "Saves the default port and clears row overrides — all rows use default again",
      "bar.pull": "Pull from Punk",
      "bar.pullTitle":
        "Reads layout + params + global from the connected Faderpunk (via Configurator Save Setup)",
      "bar.groupPreset": "Preset",
      "bar.groupPort": "MIDI Out",
      "bar.groupJson": "JSON",
      "bar.export": "JSON ↓",
      "bar.exportAll": "ZIP all JSON",
      "bar.cheatsheet": "Print cheatsheet ✎",
      "bar.cheatsheetTitle":
        "Print 16 faders as a strip (20 cm) along the top of A4 — number, instrument, app, CC",
      "bar.copy": "Copy JSON",
      "bar.jsonImport": "JSON → active preset",
      "port.out1": "Out1",
      "port.out2": "Out2",
      "port.usb": "USB",
      "port.all": "USB+Out1+Out2",
      "port.default": "Default",

      "global.summary": "Presettings",
      "global.quantizer": "Quantizer",
      "global.scale": "Scale (Key)",
      "global.tonic": "Tonic",
      "global.clock": "Clock",
      "global.clockSrc": "Clock source",
      "global.resetSrc": "Reset source",
      "global.bpm": "BPM",
      "global.swing": "Swing (−35…+35)",
      "global.swingTitle": "0 = straight, Deluge-style, 16th-note level",
      "global.takeover": "Takeover",
      "global.takeoverTitle":
        "How faders take control when switching layers / device scenes",
      "global.aux": "AUX clocks",
      "global.atom": "Atom",
      "global.atomDiv": "Atom Div",
      "global.meteor": "Meteor",
      "global.meteorDiv": "Meteor Div",
      "global.cube": "Cube",
      "global.cubeDiv": "Cube Div",

      "dock.ready": "ready",
      "dock.logToggle": "Log ▴",
      "dock.logToggleOpen": "Log ▾",
      "dock.logToggleTitle": "Expand / collapse push log",
      "dock.pushLogEmpty":
        "No push yet. If something fails: check the Configurator Chrome window (Connect Device).",
      "dock.stop": "Stop",
      "dock.next": "Continue →",

      "table.dragTitle": "Drag to reorder",
      "table.fader": "Fader",
      "table.app": "App",
      "table.comment": "Comment",
      "table.instrument": "Instrument",
      "table.ch": "MIDI CH",
      "table.ccNote": "MIDI CC / Note",
      "table.led": "LED",
      "table.port": "MIDI Out",

      "row.add": "+ Fader",
      "row.addTitle": "Append a Control app (1 fader)",
      "row.trim": "Trim to 16",
      "row.trimTitle":
        "Delete overflow rows from the end (push already ignores them)",
      "row.remove": "Remove row",
      "row.drag": "Drag (also into/out of overflow)",
      "row.overflowPill":
        "Does not fit in 1–16 — drag up or shrink/remove the app",
      "row.overflowShort": "16+",
      "row.overflowNeed": "16+ · {need}",
      "row.overflowSep":
        "Overflow — not in push · park here / drag up",
      "row.portDefault": "default ({port})",

      "preview.summary": "Push preview (JSON)",
      "preview.metaCollapsed": "collapsed · check here on errors",
      "preview.validPush": "valid · Push = 1–16 · {n} overflow stay editor-only",
      "preview.validJson": "valid · JSON that push would send",
      "preview.invalid": "invalid · push would fail",

      "modal.title": "Notice",
      "modal.confirm": "Confirm",
      "modal.ok": "OK",
      "modal.cancel": "Cancel",
      "modal.continue": "Continue",
      "modal.stop": "Stop",
      "modal.skip": "Skip →",
      "modal.read": "Read",
      "modal.start": "Start",

      "status.presetAdded": "Preset {name} added",
      "status.presetDuplicated": 'Duplicated as "{name}"',
      "status.presetMinOne": "At least one preset must remain",
      "status.presetRemoved": 'Preset "{name}" removed · {n} left',
      "status.presetRemoveConfirm": 'Really remove preset "{name}"?',
      "status.allRowsPort": "All {n} rows → port {port}",
      "status.overridesCleared":
        "{n} port overrides cleared — all use default ({port})",
      "status.noOverrides": "No overrides — default is {port}",
      "status.overflow":
        "Preset {index} · {name} · {used}/16 active · {overflow} in overflow · {portNote}{ch16Note}",
      "status.normal":
        "Preset {index} · {name} · {used}/16 faders · {apps} apps · {portNote}{ch16Note}",
      "status.portNoteOverride": "default {port} · {n}× row port",
      "status.portNoteAll": "Port {port} (all rows)",
      "status.ch16Note": " · {n}× CH16 (Configurator load bug → push fixes)",
      "status.hintOverflow":
        "Overflow stays in the editor — push only sends 1–16",
      "status.hintFree": "{n} free",
      "status.defaultPortMixed":
        "Default port → {port}. {n} rows have their own port (ignore default). Use “Apply to all”.",
      "status.defaultPortAll": "Default port → {port} (applies to all rows)",
      "status.storageFull":
        "Browser storage full/blocked — saving to server…",
      "status.copied": "JSON copied to clipboard",
      "status.bankFromServer":
        "Bank from server · {n} presets · tab {tab}",
      "status.jsonImport": "JSON → preset {index} ({n} apps)",
      "status.trimDone":
        "{n} overflow/excess row(s) removed → 16/16",
      "status.trimOk": "Already ≤16 — filled with Control if needed",
      "status.resetConfirm": "Reset preset {index} to default?",
      "status.chromeOk":
        "Local Configurator open — Connect Device there, Push/Pull here",
      "status.waitContinue": "Waiting for Continue…",
      "status.waitPreset": 'Preset "{name}" live — Continue = load next preset',
      "status.stoppedPreset": "Stopped at preset {index}.",
      "status.stoppedLayout": 'Stopped. Active layout: "{name}".',
      "status.tourDone":
        "Tour done — last preset is the current layout.",
      "status.pullOk":
        "Preset {index} ← device ({n} apps). Tweak, then push.",
      "status.pullOkNew":
        "New preset {index} ← device ({n} apps).",
      "status.noExport": "No presets to export",
      "status.zipDone":
        "ZIP ready: {n} presets → faderpunk-presets-{n}.zip",

      "warn.noParams":
        "Preset {index}: {app} (layoutId {id}) has no params — MIDI would be missing",
      "warn.fewParams":
        "Preset {index}: {app} has {have}/{min} params — firmware would use defaults",
      "warn.noMidiOut":
        "Preset {index}: layoutId {id} ({app}) has no MidiOut — port would become default",

      "push.metaChrome": "chrome…",
      "push.metaChromeOk": "chrome ok",
      "push.metaChromeErr": "chrome error",
      "push.metaRunning": "running…",
      "push.metaPull": "pull…",
      "push.openLocal": "=== Open Local Configurator ===",
      "push.openLocalTitle": "Open Local Configurator",
      "push.openLocalFail": "Open Local Configurator failed",
      "push.openLocalOk":
        "Configurator Chrome is ready (dedicated profile).\n\n1. Tab “Local Configurator” → Connect Device (once).\n2. Keep using Push/Pull in this editor — automation attaches to that Chrome.\n\nURL: http://127.0.0.1:5173/#/configurator\n\nVite dev server must be running (pnpm -C configurator dev).\nIf MIDI fails: close other Chrome windows that own the Faderpunk.",
      "push.failHttp": "Push failed (HTTP {status})",
      "push.failTitle": "Push failed",
      "push.failNoResponse": "Push: no response from server",
      "push.failAlert":
        "{msg}\n\n• Server running? (npm start)\n• Check push log below",
      "push.doneSec": "✓ done in {sec}s",
      "push.liveOverflow":
        'Preset "{name}" live · {n} overflow editor-only',
      "push.liveOk":
        'Preset "{name}" live — device scenes store values only, not apps/CCs',
      "push.failTips":
        "{msg}\n\nTips:\n• Server running? (npm start)\n• Chrome window “127.0.0.1:5173” → Connect Device\n• Check push log below",
      "pull.confirm":
        "Read current layout + params + global config from the Faderpunk?\n\nNew preset = keep the active one.\nReplace = overwrite the active preset.\n\nDevice must be connected in the Configurator.",
      "pull.asNew": "New preset",
      "pull.replace": "Replace",
      "pull.firstRun":
        "This editor starts empty — no studio presets are shipped.\n\nRead the current layout from your Faderpunk now?\n\nDevice must be connected in the Configurator.",
      "pull.title": "Pull from Punk",
      "status.emptyBank": "Empty editor — add rows or read from device",
      "modal.later": "Not now",
      "pull.logStart": "=== Pull from device ===",
      "pull.okLog":
        "✓ {n} apps + global applied to preset {index}",
      "pull.okLogNew":
        "✓ {n} apps + global → new preset {index}",
      "pull.failTitle": "Pull failed",
      "pull.failHttp": "Pull failed (HTTP {status})",
      "pull.failAlert":
        "{msg}\n\n• Server running? (npm start)\n• Check push log",
      "pull.failTips":
        "{msg}\n\nTips:\n• Configurator open, device connected\n• Layout with at least one app\n• Check push log",
      "tour.confirm":
        "Load all {n} presets into the Configurator one by one?\n\nImportant: This overwrites the global layout every time.\nHardware scenes do not store CC/app assignments — only values.\n\nUseful for auditioning; for a gig, push the matching preset when you switch.",
      "tour.title": "Presets one by one",
      "tour.logStart": "=== Preset tour start ===",
      "tour.skip": "skip preset {index}: {msg}",
      "tour.invalidHtml":
        "<strong>Preset {index} invalid</strong> — {msg}<br>Continue = skip",
      "tour.stoppedInvalid": "stopped at invalid preset {index}",
      "tour.waitLog": "waiting for Continue (preset {index} active)",
      "tour.stoppedKeep": 'stopped — layout "{name}" stays active',
      "tour.logDone": "=== Tour done ===",
      "tour.doneAlert":
        "Done. Reminder: for the gig, push the matching preset when you switch — don’t expect device scenes with different assignments.",
      "tour.doneTitle": "Preset tour",
      "tour.failTitle": "Push error",
      "tour.failAlert":
        "{msg}\n\nSee push log for Playwright steps.",
      "tour.nextHint": "<br>Continue → loads the next preset (overwrites layout).",
      "cheatsheet.preset": "Preset {index}",
      "midi.gridsChTitle": "FP Grids: 4 MIDI channels",
      "midi.gridsNoteTitle": "FP Grids: 4 notes (BD/SD/HH/X)",
      "midi.groovesChTitle": "Grooves: Kick/Snare/Hats MIDI channels",
      "midi.groovesNoteTitle": "Grooves: Kick/Snare/Hats notes",
      "midi.chLabel": "{lab} MIDI CH",
      "midi.noteLabel": "{lab} Note",
      "midi.modeTitle": "MIDI Mode: Note = notes (no CC), Cc = CC only",
      "param.lfoSpeed": "LFO Speed",
      "param.groove": "Groove / genre",
      "param.swingHint": "Max swing as % of a 16th when fader is full",
      "param.cvRange": "CV Range",
      "param.mixMode": "Mix Mode",
      "param.oscB": "Osc B",
      "param.mixBalance": "Mix %",
      "param.mixBalanceHint": "Mix balance −100 (A) … 0 (center) … +100 (B)",
      "param.gateSpeed": "Gate Speed",
      "param.vpo": "V/Oct",
      "param.bypassQ": "Bypass quantizer",
      "param.bypassQShort": "Bypass Q",
      "desc.portOverrides": " — {n} row port override(s)",
      "dock.done": "Done",
      "dock.waitContinue": "Waiting for Continue…",
      "inst.nameMissing": "Name required",
      "load.failed": "Load failed: {msg}",
    },

    de: {
      "doc.title": "Faderpunk Preset Editor",
      "app.h1": "Preset Editor",
      "app.compat":
        "Für den <a href=\"https://faderpunk.io/beta/#/configurator\" target=\"_blank\" rel=\"noopener noreferrer\">faderpunk.io/beta</a>-Configurator",
      "lang.label": "Sprache",

      "push.fabTitle": "Aktives Preset auf den Faderpunk pushen",
      "push.fabLine1": "Push",
      "push.fabLine2": "to Punk",

      "inst.summary": "Deine MIDI-Instrumente",
      "inst.groupNew": "Neu",
      "inst.groupList": "Bibliothek",
      "inst.empty": "Noch keine Instrumente.",
      "inst.remove": "Entfernen",
      "inst.instrument": "Instrument",
      "inst.name": "Name",
      "inst.midiCh": "CH",
      "inst.csv": "CC-CSV",
      "inst.recent": "Zuletzt verwendet",
      "inst.all": "Alle Instrumente",
      "inst.csvNone": "wählen (oder nicht)",
      "inst.csvEmpty": "— keine CSV —",
      "inst.manual": "— manuell —",
      "inst.add": "Instrument hinzufügen",
      "inst.csvUpload": "CSV hochladen",
      "inst.csvUploadTitle":
        "CC/NRPN-CSV nach midi-custom/ legen (erscheint als Custom/… in der Liste)",
      "inst.csvUploadOk": "CSV hochgeladen: {path}",
      "inst.csvUploadFail": "CSV-Upload fehlgeschlagen: {msg}",
      "inst.midiSync": "CCs online laden",
      "inst.midiSyncTitle":
        "Lädt die MIDI-CC/NRPN-CSV-Bibliothek von midi.guide (pencilresearch/midi auf GitHub). Hochgeladene Dateien in midi-custom/ bleiben.",
      "inst.midiSyncOk": "Online-CCs: {message}",
      "inst.midiSyncFail": "Online-CC-Laden fehlgeschlagen: {msg}",
      "inst.removeConfirm": 'Instrument „{name}“ entfernen?',
      "inst.chUpdated":
        'Instrument „{name}“ → CH{ch} · {n} Zeile(n) aktualisiert',

      "tabs.aria": "Presets",
      "tabs.remove": "Preset entfernen",
      "tabs.add": "Preset hinzufügen",
      "tabs.removeAria": "Entfernen",
      "tabs.addAria": "Hinzufügen",
      "tabs.dup": "Preset duplizieren",
      "tabs.dupAria": "Duplizieren",
      "tabs.io": "Export / Import",

      "bar.shortName": "Kurzname",
      "bar.desc": "Beschreibung",
      "bar.defaultPort": "Default-Port",
      "bar.defaultPortTitle":
        "Gilt für Zeilen mit Port „default“. Zeilen mit eigenem Port ignorieren das.",
      "bar.portAll": "Auf alle anwenden",
      "bar.portAllTitle":
        "Speichert den Default-Port und löscht Zeilen-Overrides — alle nutzen wieder Default",
      "bar.pull": "Pull from Punk",
      "bar.pullTitle":
        "Liest Layout+Params+Global vom verbundenen Faderpunk (via Configurator Save Setup)",
      "bar.groupPreset": "Preset",
      "bar.groupPort": "MIDI Out",
      "bar.groupJson": "JSON",
      "bar.export": "JSON ↓",
      "bar.exportAll": "ZIP all JSON",
      "bar.cheatsheet": "Cheatsheet drucken ✎",
      "bar.cheatsheetTitle":
        "16 Fader als Streifen (20 cm) am oberen A4-Rand drucken — Nummer, Instrument, App, CC",
      "bar.copy": "JSON kopieren",
      "bar.jsonImport": "JSON → aktives Preset",
      "port.out1": "Out1",
      "port.out2": "Out2",
      "port.usb": "USB",
      "port.all": "USB+Out1+Out2",
      "port.default": "Standard",

      "global.summary": "Presettings",
      "global.quantizer": "Quantizer",
      "global.scale": "Scale (Key)",
      "global.tonic": "Tonic",
      "global.clock": "Clock",
      "global.clockSrc": "Clock source",
      "global.resetSrc": "Reset source",
      "global.bpm": "BPM",
      "global.swing": "Swing (−35…+35)",
      "global.swingTitle": "0 = straight, Deluge-style, 16th-note level",
      "global.takeover": "Takeover",
      "global.takeoverTitle":
        "How faders take control when switching layers / device scenes",
      "global.aux": "AUX clocks",
      "global.atom": "Atom",
      "global.atomDiv": "Atom Div",
      "global.meteor": "Meteor",
      "global.meteorDiv": "Meteor Div",
      "global.cube": "Cube",
      "global.cubeDiv": "Cube Div",

      "dock.ready": "bereit",
      "dock.logToggle": "Log ▴",
      "dock.logToggleOpen": "Log ▾",
      "dock.logToggleTitle": "Push-Log ein-/ausklappen",
      "dock.pushLogEmpty":
        "Noch kein Push. Bei Problemen: Chrome-Fenster vom Configurator prüfen (Connect Device).",
      "dock.stop": "Stop",
      "dock.next": "Weiter →",

      "table.dragTitle": "Ziehen zum Umsortieren",
      "table.fader": "Fader",
      "table.app": "App",
      "table.comment": "Kommentar",
      "table.instrument": "Instrument",
      "table.ch": "MIDI CH",
      "table.ccNote": "MIDI CC / Note",
      "table.led": "LED",
      "table.port": "MIDI Out",

      "row.add": "+ Fader",
      "row.addTitle": "Eine Control-App (1 Fader) ans Ende",
      "row.trim": "Auf 16 kürzen",
      "row.trimTitle":
        "Overflow-Zeilen von hinten löschen (Push ignoriert sie ohnehin)",
      "row.remove": "Zeile entfernen",
      "row.drag": "Ziehen (auch in/aus Overflow)",
      "row.overflowPill":
        "Passt nicht in 1–16 — nach oben ziehen oder App verkleinern/entfernen",
      "row.overflowShort": "16+",
      "row.overflowNeed": "16+ · {need}",
      "row.overflowSep":
        "Overflow — nicht im Push · hier parken / nach oben ziehen",
      "row.portDefault": "default ({port})",

      "preview.summary": "Push-Vorschau (JSON)",
      "preview.metaCollapsed": "zugeklappt · bei Fehler hier prüfen",
      "preview.validPush":
        "gültig · Push = 1–16 · {n} Overflow bleiben nur im Editor",
      "preview.validJson": "gültig · JSON das Push senden würde",
      "preview.invalid": "ungültig · Push würde scheitern",

      "modal.title": "Hinweis",
      "modal.confirm": "Bestätigen",
      "modal.ok": "OK",
      "modal.cancel": "Abbrechen",
      "modal.continue": "Weiter",
      "modal.stop": "Stop",
      "modal.skip": "Überspringen →",
      "modal.read": "Lesen",
      "modal.start": "Start",

      "status.presetAdded": "Preset {name} hinzugefügt",
      "status.presetDuplicated": 'Dupliziert als „{name}“',
      "status.presetMinOne": "Mindestens ein Preset muss bleiben",
      "status.presetRemoved":
        'Preset „{name}“ entfernt · {n} übrig',
      "status.presetRemoveConfirm":
        'Preset „{name}“ wirklich entfernen?',
      "status.allRowsPort": "Alle {n} Zeilen → Port {port}",
      "status.overridesCleared":
        "{n} Port-Overrides gelöscht — alle nutzen Default ({port})",
      "status.noOverrides": "Keine Overrides — Default ist {port}",
      "status.overflow":
        "Preset {index} · {name} · {used}/16 aktiv · {overflow} im Overflow · {portNote}{ch16Note}",
      "status.normal":
        "Preset {index} · {name} · {used}/16 Fader · {apps} Apps · {portNote}{ch16Note}",
      "status.portNoteOverride": "default {port} · {n}× Zeilen-Port",
      "status.portNoteAll": "Port {port} (alle Zeilen)",
      "status.ch16Note":
        " · {n}× CH16 (Configurator-Load bug → Push zieht nach)",
      "status.hintOverflow":
        "Overflow bleibt im Editor — Push sendet nur 1–16",
      "status.hintFree": "{n} frei",
      "status.defaultPortMixed":
        "Default-Port → {port}. {n} Zeilen haben eigenen Port (ignorieren Default). „Auf alle anwenden“.",
      "status.defaultPortAll":
        "Default-Port → {port} (gilt für alle Zeilen)",
      "status.storageFull":
        "Browser-Speicher voll/gesperrt — speichere auf Server…",
      "status.copied": "JSON in Zwischenablage",
      "status.bankFromServer":
        "Bank vom Server · {n} Presets · Tab {tab}",
      "status.jsonImport": "JSON → Preset {index} ({n} Apps)",
      "status.trimDone":
        "{n} Overflow-/Überschuss-Zeile(n) entfernt → 16/16",
      "status.trimOk":
        "Bereits ≤16 — aufgefüllt mit Control falls nötig",
      "status.resetConfirm":
        "Preset {index} auf Default zurücksetzen?",
      "status.chromeOk":
        "Local Configurator offen — Connect Device dort, Push/Pull weiter hier",
      "status.waitContinue": "Warte auf Weiter…",
      "status.waitPreset":
        'Preset „{name}“ live — Weiter = nächstes Preset laden',
      "status.stoppedPreset": "Gestoppt bei Preset {index}.",
      "status.stoppedLayout":
        'Gestoppt. Aktives Layout: „{name}“.',
      "status.tourDone":
        "Rundgang fertig — letztes Preset ist das aktuelle Layout.",
      "status.pullOk":
        "Preset {index} ← Gerät ({n} Apps). Tweaken, dann pushen.",
      "status.pullOkNew":
        "Neues Preset {index} ← Gerät ({n} Apps).",
      "status.noExport": "Keine Presets exportierbar",
      "status.zipDone":
        "ZIP geladen: {n} Presets → faderpunk-presets-{n}.zip",

      "warn.noParams":
        "Preset {index}: {app} (layoutId {id}) ohne Params — MIDI würde fehlen",
      "warn.fewParams":
        "Preset {index}: {app} hat {have}/{min} Params — Firmware würde Defaults nutzen",
      "warn.noMidiOut":
        "Preset {index}: layoutId {id} ({app}) ohne MidiOut — Port würde Default werden",

      "push.metaChrome": "chrome…",
      "push.metaChromeOk": "chrome ok",
      "push.metaChromeErr": "chrome fehler",
      "push.metaRunning": "läuft…",
      "push.metaPull": "pull…",
      "push.openLocal": "=== Open Local Configurator ===",
      "push.openLocalTitle": "Open Local Configurator",
      "push.openLocalFail": "Open Local Configurator fehlgeschlagen",
      "push.openLocalOk":
        "Configurator-Chrome ist bereit (eigenes Profil).\n\n• Server läuft? (npm start)\n• Push-Log unten prüfen",
      "push.failHttp": "Push fehlgeschlagen (HTTP {status})",
      "push.failTitle": "Push fehlgeschlagen",
      "push.failNoResponse": "Push: keine Antwort vom Server",
      "push.failAlert":
        "{msg}\n\n• Server läuft? (npm start)\n• Push-Log unten prüfen",
      "push.doneSec": "✓ fertig in {sec}s",
      "pull.confirm":
        "Aktuelles Layout + Params + Global Config vom Faderpunk lesen?\n\nNeues Preset = aktives behalten.\nErsetzen = aktives Preset überschreiben.\n\nGerät muss im Configurator verbunden sein.",
      "pull.asNew": "Neues Preset",
      "pull.replace": "Ersetzen",
      "pull.firstRun":
        "Der Editor startet leer — es werden keine Studio-Presets mitgeliefert.\n\nAktuelles Layout jetzt vom Faderpunk lesen?\n\nGerät muss im Configurator verbunden sein.",
      "pull.title": "Pull from Punk",
      "status.emptyBank": "Leerer Editor — Zeilen hinzufügen oder vom Gerät lesen",
      "modal.later": "Später",
      "pull.logStart": "=== Pull vom Gerät ===",
      "pull.okLog":
        "✓ {n} Apps + Global ins Preset {index} übernommen",
      "pull.okLogNew":
        "✓ {n} Apps + Global → neues Preset {index}",
      "pull.failTitle": "Pull fehlgeschlagen",
      "pull.failHttp": "Pull fehlgeschlagen (HTTP {status})",
      "pull.failAlert":
        "{msg}\n\n• Server läuft? (npm start)\n• Push-Log prüfen",
      "tour.confirm":
        "Alle {n} Presets nacheinander in den Configurator laden?\n\nWichtig: Das überschreibt jedes Mal das globale Layout.\nHardware-Scenes speichern keine CC/App-Zuweisungen — nur Werte.\n\nSinnvoll zum Durchprobieren; für den Gig das passende Preset pushen, wenn du wechselst.",
      "tour.title": "Presets nacheinander",
      "tour.logStart": "=== Preset-Rundgang starten ===",
      "tour.skip": "skip Preset {index}: {msg}",
      "tour.invalidHtml":
        "<strong>Preset {index} ungültig</strong> — {msg}<br>Weiter = überspringen",
      "tour.stoppedInvalid":
        "gestoppt bei ungültigem Preset {index}",
      "tour.waitLog":
        "warte auf Weiter (Preset {index} aktiv)",
      "tour.stoppedKeep":
        'gestoppt — Layout „{name}“ bleibt aktiv',
      "tour.logDone": "=== Rundgang fertig ===",
      "tour.doneAlert":
        "Fertig. Merke: Für den Gig das passende Preset pushen, wenn du wechselst — nicht Device-Scenes mit verschiedenen Assignments erwarten.",
      "tour.doneTitle": "Preset-Rundgang",
      "tour.failTitle": "Push-Fehler",
      "tour.failAlert":
        "{msg}\n\nSiehe Push-Log für Playwright-Schritte.",
      "tour.nextHint":
        "<br>Weiter → lädt das nächste Preset (überschreibt Layout).",
      "cheatsheet.preset": "Preset {index}",
      "midi.gridsChTitle": "FP Grids: 4 MIDI-Kanäle",
      "midi.gridsNoteTitle": "FP Grids: 4 Noten (BD/SD/HH/X)",
      "midi.groovesChTitle": "Grooves: Kick/Snare/Hats MIDI-Kanäle",
      "midi.groovesNoteTitle": "Grooves: Kick/Snare/Hats Noten",
      "midi.chLabel": "{lab} MIDI CH",
      "midi.noteLabel": "{lab} Note",
      "midi.modeTitle":
        "MIDI Mode: Note = Noten (kein CC), Cc = nur CC",
      "param.lfoSpeed": "LFO Speed",
      "param.groove": "Groove / genre",
      "param.swingHint":
        "Max. Swing als % einer 16tel bei voll ausgeschlagenem Fader",
      "param.cvRange": "CV Range",
      "param.mixMode": "Mix-Modus",
      "param.oscB": "Osc B",
      "param.mixBalance": "Mix %",
      "param.mixBalanceHint": "Mix-Balance −100 (A) … 0 (Mitte) … +100 (B)",
      "param.gateSpeed": "Gate-Speed",
      "param.vpo": "V/Oct",
      "param.bypassQ": "Quantizer umgehen",
      "param.bypassQShort": "Bypass Q",
      "desc.portOverrides": " — {n} Zeilen-Port-Override(s)",
      "push.liveOverflow":
        'Preset „{name}“ live · {n} Overflow nur im Editor',
      "push.liveOk":
        'Preset „{name}“ live — Device-Scenes speichern nur Werte, nicht Apps/CCs',
      "push.failTips":
        "{msg}\n\nTipps:\n• Server läuft? (npm start)\n• Chrome-Fenster „127.0.0.1:5173“ → Connect Device\n• Push-Log unten prüfen",
      "pull.failTips":
        "{msg}\n\nTipps:\n• Configurator offen, Device verbunden\n• Layout mit mind. einer App\n• Push-Log prüfen",
      "dock.done": "Fertig",
      "dock.waitContinue": "Warte auf Weiter…",
      "inst.nameMissing": "Name fehlt",
      "load.failed": "Laden fehlgeschlagen: {msg}",
      "push.openLocalOk":
        "Configurator-Chrome ist bereit (eigenes Profil).\n\n1. Tab „Local Configurator“ → Connect Device (einmal).\n2. Push/Pull weiterhin in diesem Editor — Automation dockt an dieses Chrome an.\n\nURL: http://127.0.0.1:5173/#/configurator\n\nVite-Dev-Server muss laufen (pnpm -C configurator dev).\nFalls MIDI nicht greift: anderen Chrome mit dem Faderpunk schließen.",
    },

    fr: {
      "doc.title": "Éditeur de presets Faderpunk",
      "app.h1": "Éditeur de presets",
      "app.compat":
        "Compatible avec le configurateur <a href=\"https://faderpunk.io/beta/#/configurator\" target=\"_blank\" rel=\"noopener noreferrer\">faderpunk.io/beta</a>",
      "lang.label": "Langue",

      "push.fabTitle": "Envoyer le preset actif au Faderpunk",
      "push.fabLine1": "Push",
      "push.fabLine2": "to Punk",

      "inst.summary": "Vos instruments MIDI",
      "inst.groupNew": "Nouveau",
      "inst.groupList": "Bibliothèque",
      "inst.empty": "Aucun instrument.",
      "inst.remove": "Supprimer",
      "inst.instrument": "Instrument",
      "inst.name": "Nom",
      "inst.midiCh": "CH",
      "inst.csv": "CSV CC",
      "inst.recent": "Utilisés récemment",
      "inst.all": "Tous les instruments",
      "inst.csvNone": "choisir (ou pas)",
      "inst.csvEmpty": "— pas de CSV —",
      "inst.manual": "— manuel —",
      "inst.add": "Ajouter un instrument",
      "inst.csvUpload": "Importer CSV",
      "inst.csvUploadTitle":
        "Ajouter un CSV CC/NRPN dans midi-custom/ (apparaît comme Custom/… dans la liste)",
      "inst.csvUploadOk": "CSV importé : {path}",
      "inst.csvUploadFail": "Échec import CSV : {msg}",
      "inst.midiSync": "Charger CC en ligne",
      "inst.midiSyncTitle":
        "Télécharge la bibliothèque CSV MIDI CC/NRPN depuis midi.guide (pencilresearch/midi sur GitHub). Vos fichiers midi-custom/ restent.",
      "inst.midiSyncOk": "CC en ligne : {message}",
      "inst.midiSyncFail": "Échec chargement CC en ligne : {msg}",
      "inst.removeConfirm": 'Supprimer l’instrument « {name} » ?',
      "inst.chUpdated":
        'Instrument « {name} » → CH{ch} · {n} ligne(s) mise(s) à jour',

      "tabs.aria": "Presets",
      "tabs.remove": "Supprimer le preset",
      "tabs.add": "Ajouter un preset",
      "tabs.removeAria": "Supprimer",
      "tabs.addAria": "Ajouter",
      "tabs.dup": "Dupliquer le preset",
      "tabs.dupAria": "Dupliquer",
      "tabs.io": "Export / Import",

      "bar.shortName": "Nom court",
      "bar.desc": "Description",
      "bar.defaultPort": "Port par défaut",
      "bar.defaultPortTitle":
        'S’applique aux lignes avec le port « default ». Les lignes avec leur propre port l’ignorent.',
      "bar.portAll": "Appliquer à tous",
      "bar.portAllTitle":
        "Enregistre le port par défaut et efface les overrides — toutes les lignes utilisent le défaut",
      "bar.pull": "Pull from Punk",
      "bar.pullTitle":
        "Lit layout + params + global depuis le Faderpunk connecté (via Configurator Save Setup)",
      "bar.groupPreset": "Preset",
      "bar.groupPort": "MIDI Out",
      "bar.groupJson": "JSON",
      "bar.export": "JSON ↓",
      "bar.exportAll": "ZIP all JSON",
      "bar.cheatsheet": "Imprimer l’aide-mémoire ✎",
      "bar.cheatsheetTitle":
        "Imprimer 16 faders en bande (20 cm) en haut d’un A4 — numéro, instrument, app, CC",
      "bar.copy": "Copier JSON",
      "bar.jsonImport": "JSON → preset actif",
      "port.out1": "Out1",
      "port.out2": "Out2",
      "port.usb": "USB",
      "port.all": "USB+Out1+Out2",
      "port.default": "Défaut",

      "global.summary": "Presettings",
      "global.quantizer": "Quantizer",
      "global.scale": "Scale (Key)",
      "global.tonic": "Tonic",
      "global.clock": "Clock",
      "global.clockSrc": "Source clock",
      "global.resetSrc": "Source reset",
      "global.bpm": "BPM",
      "global.swing": "Swing (−35…+35)",
      "global.swingTitle": "0 = straight, style Deluge, niveau 16e",
      "global.takeover": "Takeover",
      "global.takeoverTitle":
        "Comment les faders prennent le contrôle lors du changement de couche / scène",
      "global.aux": "Clocks AUX",
      "global.atom": "Atom",
      "global.atomDiv": "Atom Div",
      "global.meteor": "Meteor",
      "global.meteorDiv": "Meteor Div",
      "global.cube": "Cube",
      "global.cubeDiv": "Cube Div",

      "dock.ready": "prêt",
      "dock.logToggle": "Log ▴",
      "dock.logToggleOpen": "Log ▾",
      "dock.logToggleTitle": "Afficher / masquer le journal push",
      "dock.pushLogEmpty":
        "Pas encore de push. En cas de problème : vérifier la fenêtre Chrome du Configurator (Connect Device).",
      "dock.stop": "Stop",
      "dock.next": "Continuer →",

      "table.dragTitle": "Glisser pour réordonner",
      "table.fader": "Fader",
      "table.app": "App",
      "table.comment": "Commentaire",
      "table.instrument": "Instrument",
      "table.ch": "MIDI CH",
      "table.ccNote": "MIDI CC / Note",
      "table.led": "LED",
      "table.port": "MIDI Out",

      "row.add": "+ Fader",
      "row.addTitle": "Ajouter une app Control (1 fader) à la fin",
      "row.trim": "Réduire à 16",
      "row.trimTitle":
        "Supprimer les lignes overflow depuis la fin (le push les ignore déjà)",
      "row.remove": "Supprimer la ligne",
      "row.drag": "Glisser (aussi dans/hors overflow)",
      "row.overflowPill":
        "Ne tient pas dans 1–16 — glisser vers le haut ou réduire/supprimer l’app",
      "row.overflowShort": "16+",
      "row.overflowNeed": "16+ · {need}",
      "row.overflowSep":
        "Overflow — pas dans le push · garer ici / glisser vers le haut",
      "row.portDefault": "default ({port})",

      "preview.summary": "Aperçu push (JSON)",
      "preview.metaCollapsed": "replié · vérifier ici en cas d’erreur",
      "preview.validPush":
        "valide · Push = 1–16 · {n} overflow restent dans l’éditeur",
      "preview.validJson": "valide · JSON que le push enverrait",
      "preview.invalid": "invalide · le push échouerait",

      "modal.title": "Notice",
      "modal.confirm": "Confirmer",
      "modal.ok": "OK",
      "modal.cancel": "Annuler",
      "modal.continue": "Continuer",
      "modal.stop": "Stop",
      "modal.skip": "Ignorer →",
      "modal.read": "Lire",
      "modal.start": "Démarrer",

      "status.presetAdded": "Preset {name} ajouté",
      "status.presetDuplicated": 'Dupliqué en « {name} »',
      "status.presetMinOne": "Au moins un preset doit rester",
      "status.presetRemoved":
        'Preset « {name} » supprimé · {n} restant(s)',
      "status.presetRemoveConfirm":
        'Vraiment supprimer le preset « {name} » ?',
      "status.allRowsPort": "Toutes les {n} lignes → port {port}",
      "status.overridesCleared":
        "{n} overrides de port effacés — toutes utilisent le défaut ({port})",
      "status.noOverrides": "Pas d’overrides — défaut = {port}",
      "status.overflow":
        "Preset {index} · {name} · {used}/16 actifs · {overflow} en overflow · {portNote}{ch16Note}",
      "status.normal":
        "Preset {index} · {name} · {used}/16 faders · {apps} apps · {portNote}{ch16Note}",
      "status.portNoteOverride": "default {port} · {n}× port de ligne",
      "status.portNoteAll": "Port {port} (toutes les lignes)",
      "status.ch16Note":
        " · {n}× CH16 (bug load Configurator → le push corrige)",
      "status.hintOverflow":
        "L’overflow reste dans l’éditeur — le push n’envoie que 1–16",
      "status.hintFree": "{n} libre(s)",
      "status.defaultPortMixed":
        "Port par défaut → {port}. {n} lignes ont leur propre port (ignorent le défaut). « Appliquer à tous ».",
      "status.defaultPortAll":
        "Port par défaut → {port} (toutes les lignes)",
      "status.storageFull":
        "Stockage navigateur plein/bloqué — enregistrement serveur…",
      "status.copied": "JSON copié dans le presse-papiers",
      "status.bankFromServer":
        "Banque serveur · {n} presets · onglet {tab}",
      "status.jsonImport": "JSON → preset {index} ({n} apps)",
      "status.trimDone":
        "{n} ligne(s) overflow/excédent supprimée(s) → 16/16",
      "status.trimOk":
        "Déjà ≤16 — complété avec Control si besoin",
      "status.resetConfirm":
        "Réinitialiser le preset {index} au défaut ?",
      "status.chromeOk":
        "Configurator local ouvert — Connect Device là-bas, Push/Pull ici",
      "status.waitContinue": "En attente de Continuer…",
      "status.waitPreset":
        'Preset « {name} » actif — Continuer = charger le suivant',
      "status.stoppedPreset": "Arrêté au preset {index}.",
      "status.stoppedLayout":
        'Arrêté. Layout actif : « {name} ».',
      "status.tourDone":
        "Tour terminé — le dernier preset est le layout actuel.",
      "status.pullOk":
        "Preset {index} ← appareil ({n} apps). Ajuster, puis push.",
      "status.pullOkNew":
        "Nouveau preset {index} ← appareil ({n} apps).",
      "status.noExport": "Aucun preset à exporter",
      "status.zipDone":
        "ZIP prêt : {n} presets → faderpunk-presets-{n}.zip",

      "warn.noParams":
        "Preset {index}: {app} (layoutId {id}) sans params — MIDI manquerait",
      "warn.fewParams":
        "Preset {index}: {app} a {have}/{min} params — le firmware utiliserait les défauts",
      "warn.noMidiOut":
        "Preset {index}: layoutId {id} ({app}) sans MidiOut — le port deviendrait default",

      "push.metaChrome": "chrome…",
      "push.metaChromeOk": "chrome ok",
      "push.metaChromeErr": "erreur chrome",
      "push.metaRunning": "en cours…",
      "push.metaPull": "pull…",
      "push.openLocal": "=== Open Local Configurator ===",
      "push.openLocalTitle": "Open Local Configurator",
      "push.openLocalFail": "Échec Open Local Configurator",
      "push.openLocalOk":
        "Chrome Configurator prêt (profil dédié).\n\n• Serveur lancé ? (npm start)\n• Vérifier le journal push ci-dessous",
      "push.failHttp": "Échec du push (HTTP {status})",
      "push.failTitle": "Échec du push",
      "push.failNoResponse": "Push : pas de réponse du serveur",
      "push.failAlert":
        "{msg}\n\n• Serveur lancé ? (npm start)\n• Vérifier le journal push",
      "push.doneSec": "✓ terminé en {sec}s",
      "pull.confirm":
        "Lire le layout + params + config globale actuels depuis le Faderpunk ?\n\nNouveau preset = garder l’actif.\nRemplacer = écraser le preset actif.\n\nL’appareil doit être connecté dans le Configurator.",
      "pull.asNew": "Nouveau preset",
      "pull.replace": "Remplacer",
      "pull.firstRun":
        "L’éditeur démarre vide — aucun preset studio n’est fourni.\n\nLire le layout actuel depuis le Faderpunk maintenant ?\n\nL’appareil doit être connecté dans le Configurator.",
      "pull.title": "Pull from Punk",
      "status.emptyBank": "Éditeur vide — ajoutez des lignes ou lisez depuis l’appareil",
      "modal.later": "Plus tard",
      "pull.logStart": "=== Pull depuis l’appareil ===",
      "pull.okLog":
        "✓ {n} apps + global appliqués au preset {index}",
      "pull.okLogNew":
        "✓ {n} apps + global → nouveau preset {index}",
      "pull.failTitle": "Échec du pull",
      "pull.failHttp": "Échec du pull (HTTP {status})",
      "pull.failAlert":
        "{msg}\n\n• Serveur lancé ? (npm start)\n• Vérifier le journal",
      "tour.confirm":
        "Charger les {n} presets dans le Configurator l’un après l’autre ?\n\nImportant : cela écrase le layout global à chaque fois.\nLes scènes hardware ne stockent pas les assignations CC/app — seulement les valeurs.\n\nUtile pour auditionner ; pour le live, poussez le preset correspondant quand vous changez.",
      "tour.title": "Presets l’un après l’autre",
      "tour.logStart": "=== Début du tour de presets ===",
      "tour.skip": "ignore preset {index}: {msg}",
      "tour.invalidHtml":
        "<strong>Preset {index} invalide</strong> — {msg}<br>Continuer = ignorer",
      "tour.stoppedInvalid":
        "arrêté sur preset invalide {index}",
      "tour.waitLog":
        "en attente de Continuer (preset {index} actif)",
      "tour.stoppedKeep":
        'arrêté — layout « {name} » reste actif',
      "tour.logDone": "=== Tour terminé ===",
      "tour.doneAlert":
        "Terminé. Rappel : pour le live, poussez le preset correspondant quand vous changez — ne comptez pas sur des scènes appareil avec des assignations différentes.",
      "tour.doneTitle": "Tour de presets",
      "tour.failTitle": "Erreur de push",
      "tour.failAlert":
        "{msg}\n\nVoir le journal push pour les étapes Playwright.",
      "tour.nextHint":
        "<br>Continuer → charge le preset suivant (écrase le layout).",
      "cheatsheet.preset": "Preset {index}",
      "midi.gridsChTitle": "FP Grids : 4 canaux MIDI",
      "midi.gridsNoteTitle": "FP Grids : 4 notes (BD/SD/HH/X)",
      "midi.groovesChTitle": "Grooves : canaux MIDI Kick/Snare/Hats",
      "midi.groovesNoteTitle": "Grooves : notes Kick/Snare/Hats",
      "midi.chLabel": "{lab} CH MIDI",
      "midi.noteLabel": "{lab} Note",
      "midi.modeTitle":
        "Mode MIDI : Note = notes (pas de CC), Cc = CC seulement",
      "param.lfoSpeed": "LFO Speed",
      "param.groove": "Groove / genre",
      "param.swingHint":
        "Swing max en % d’une 16e quand le fader est à fond",
      "param.cvRange": "CV Range",
      "param.mixMode": "Mode Mix",
      "param.oscB": "Osc B",
      "param.mixBalance": "Mix %",
      "param.mixBalanceHint": "Balance mix −100 (A) … 0 (centre) … +100 (B)",
      "param.gateSpeed": "Vitesse Gate",
      "param.vpo": "V/Oct",
      "param.bypassQ": "Contourner le quantizer",
      "param.bypassQShort": "Bypass Q",
      "desc.portOverrides": " — {n} override(s) de port de ligne",
      "push.liveOverflow":
        'Preset « {name} » actif · {n} overflow éditeur seulement',
      "push.liveOk":
        'Preset « {name} » actif — les scènes appareil stockent seulement les valeurs, pas apps/CCs',
      "push.failTips":
        "{msg}\n\nConseils :\n• Serveur lancé ? (npm start)\n• Fenêtre Chrome « 127.0.0.1:5173 » → Connect Device\n• Vérifier le journal push",
      "pull.failTips":
        "{msg}\n\nConseils :\n• Configurator ouvert, appareil connecté\n• Layout avec au moins une app\n• Vérifier le journal",
      "dock.done": "Terminé",
      "dock.waitContinue": "En attente de Continuer…",
      "inst.nameMissing": "Nom requis",
      "load.failed": "Échec du chargement : {msg}",
      "push.openLocalOk":
        "Chrome Configurator prêt (profil dédié).\n\n1. Onglet « Local Configurator » → Connect Device (une fois).\n2. Continuez Push/Pull dans cet éditeur — l’automation s’attache à ce Chrome.\n\nURL : http://127.0.0.1:5173/#/configurator\n\nLe serveur Vite doit tourner (pnpm -C configurator dev).\nSi MIDI échoue : fermez les autres Chrome qui ont le Faderpunk.",
    },
  };

  let currentLang = "en";

  function detectLang() {
    try {
      const stored = localStorage.getItem(LANG_KEY);
      if (stored && SUPPORTED.includes(stored)) return stored;
    } catch {
      /* ignore */
    }
    const nav = String(
      (typeof navigator !== "undefined" &&
        (navigator.language || navigator.userLanguage)) ||
        "en",
    ).toLowerCase();
    if (nav.startsWith("de")) return "de";
    if (nav.startsWith("fr")) return "fr";
    return "en";
  }

  function t(key, vars) {
    const dict = I18N[currentLang] || I18N.en;
    let s = dict[key] ?? I18N.en[key] ?? key;
    if (vars && typeof vars === "object") {
      s = String(s).replace(/\{(\w+)\}/g, (_, k) =>
        vars[k] != null ? String(vars[k]) : `{${k}}`,
      );
    }
    return s;
  }

  function applyI18n(root) {
    const scope = root || document;
    scope.querySelectorAll("[data-i18n]").forEach((el) => {
      const key = el.getAttribute("data-i18n");
      if (!key) return;
      const val = t(key);
      if (el.tagName === "TITLE") {
        el.textContent = val;
        return;
      }
      // Preserve child elements (e.g. fab spans) — only set text if no element children
      // or if marked data-i18n-html
      if (el.hasAttribute("data-i18n-html")) {
        el.innerHTML = val;
      } else {
        el.textContent = val;
      }
    });
    scope.querySelectorAll("[data-i18n-title]").forEach((el) => {
      el.title = t(el.getAttribute("data-i18n-title"));
    });
    scope.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      el.placeholder = t(el.getAttribute("data-i18n-placeholder"));
    });
    scope.querySelectorAll("[data-i18n-aria]").forEach((el) => {
      el.setAttribute("aria-label", t(el.getAttribute("data-i18n-aria")));
    });
    document.title = t("doc.title");
    document.documentElement.lang = currentLang;
    const sel =
      typeof document.getElementById === "function"
        ? document.getElementById("langSelect")
        : null;
    if (sel && sel.value !== currentLang) sel.value = currentLang;
  }

  function setLang(lang) {
    if (!SUPPORTED.includes(lang)) lang = "en";
    currentLang = lang;
    try {
      localStorage.setItem(LANG_KEY, lang);
    } catch {
      /* ignore */
    }
    applyI18n();
    if (typeof global.afterLangChange === "function") {
      try {
        global.afterLangChange(lang);
      } catch (e) {
        console.warn("afterLangChange", e);
      }
    }
  }

  function getLang() {
    return currentLang;
  }

  currentLang = detectLang();

  global.I18N = I18N;
  global.t = t;
  global.applyI18n = applyI18n;
  global.setLang = setLang;
  global.getLang = getLang;
  global.FP_LANG_KEY = LANG_KEY;
})(typeof window !== "undefined" ? window : globalThis);
