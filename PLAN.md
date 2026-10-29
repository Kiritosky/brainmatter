# Brainmatter: Plan

Lokaler, verschlüsselter Wissens-Graph für Mac. Du und KIs (per MCP) legen einfache Einträge an,
verbinden sie thematisch und organisieren sie in verschachtelten Projekt-Graphen.
Rust-Kern, natives Mac-Fenster, schönes und schnell navigierbares UI.

---

## 1. Entscheidungen

| Thema | Entscheidung |
|---|---|
| UI-Stack | **Tauri 2 + Svelte 5** (TypeScript). Logik, DB, Suche, Krypto und MCP sind 100 % Rust. |
| KI-Zugriff | **Lokal** (Claude Code, Claude Desktop, Cursor, Codex …). Die Architektur ist **cloud-fähig**, Tunnel und OAuth kommen später. |
| Sicherheit | **Komplette Verschlüsselung** (SQLCipher, AES-256), Argon2id, **Touch ID**, Auto-Lock |
| Lock-Modus | App läuft in der **Menüleiste** weiter. Fragt eine KI an, während die App gesperrt ist, erscheint ein **Touch-ID-Prompt** („Claude Code möchte zugreifen“). |
| Apple Dev Account | **Nein**, deshalb ein selbstsigniertes Code-Signing-Zertifikat und Touch ID über LocalAuthentication (siehe §5) |
| Projekte | **Mehrfach-Zugehörigkeit**: Ein Eintrag kann in mehreren Projekten sein, Projekte sind verschachtelbar, Links über Projektgrenzen sind erlaubt |
| MVP-Extras | Semantische Suche + Auto-Links, Versionshistorie + Undo, verschlüsselte Anhänge |
| Projekt-Doku *(nach Hauptplan)* | Handbuch pro Projekt mit Lese-Ansicht, gespiegelten Doku-Ordnern und abschnittsweisem Laden für KIs (§7c) |
| Vorlagen & Prompts *(nach Hauptplan)* | Bibliothek aus Markdown-Vorlagen und Standard-Prompts, global oder pro Projekt, mit `{{Platzhaltern}}`. KIs laden sie bei Bedarf über **MCP-Prompts** (in Claude Code als Slash-Commands) oder über Tools (§7a) |
| Datei-Verknüpfungen *(nach Hauptplan)* | Einträge können auf lokale Dateien und Ordner verweisen. macOS-Bookmarks halten den Link auch nach Umbenennen oder Verschieben. Die KI sieht den Pfad und darf optional verknüpfte Dateien lesen (§7b) |
| Später | Obsidian/Markdown-Import und -Export, Cloud-KIs, Sync, iOS |

Annahmen, die ich ohne Rückfrage getroffen habe. Jede davon lässt sich ändern:
- App-Name „Brainmatter“, Bundle-ID `app.brainmatter`
- MCP-Port `127.0.0.1:47474`
- Auto-Lock: Standard nach 60 min Inaktivität und bei System-Sleep, einstellbar
- Embedding-Modell: `multilingual-e5-small` (384 Dimensionen, Deutsch und Englisch, ca. 120 MB)
- Bun als JS-Paketmanager (bereits installiert)

---

## 2. Architektur

```
┌──────────────────── Brainmatter.app (ein Prozess) ────────────────────┐
│  WebView: Svelte 5 · Sigma.js (WebGL) · CodeMirror 6                  │
│        ▲  Tauri IPC (typisiert via tauri-specta) + Events + Binär-Kanal│
│  ┌─────┴──────────────────────────────────────────────────────────┐   │
│  │ app/src-tauri : Commands · Tray/Menüleiste · LockManager       │   │
│  ├────────────────────────────────────────────────────────────────┤   │
│  │ bm-core : Vault · Store · Graph · Search · Embed · History ·   │   │
│  │           Authz · EventBus                                     │   │
│  ├────────────────────────────────────────────────────────────────┤   │
│  │ bm-mcp  : rmcp, Streamable HTTP @ 127.0.0.1:47474/mcp          │   │
│  │ bm-macos: Touch ID · Keychain · Sleep/Screen-Lock-Events       │   │
│  └────────────────────────────────────────────────────────────────┘   │
└──────────────▲───────────────────────────────────▲────────────────────┘
               │ HTTP + Bearer-Token               │ HTTP
   Claude Code · Cursor · Codex          brainmatter-mcp (stdio-Bridge)
                                                   ▲ stdio
                                     Claude Desktop & andere stdio-Clients
```

Grundprinzipien:
- **Ein Prozess, ein Schreiber.** Der Kern läuft im Tauri-Prozess, die App bleibt in der Menüleiste aktiv,
  auch wenn das Fenster geschlossen ist. Alle Schreibzugriffe laufen über einen Writer-Actor
  (tokio mpsc), Lesezugriffe über einen Pool von Read-Connections (SQLite WAL).
- **SQLite ist die Wahrheit, alles andere ist Index.** Der In-Memory-Graph (petgraph), die Vektor-Matrix
  und die Caches werden beim Entsperren aus der verschlüsselten DB geladen und beim Sperren verworfen.
- **Der EventBus** (`tokio::broadcast<DomainEvent>`) speist die Live-Updates im UI: Was eine KI schreibt,
  erscheint sofort im Graph. Außerdem verschickt er MCP-Notifications (`resources/updated`).
- **Die Rechte liegen im Kern**, nicht in der MCP-Schicht. Jede Abfrage läuft mit einem `Actor`
  (User oder KI-Token) samt Scopes, sodass Projekt-Einschränkungen überall gelten.
- **Cloud-fähig**: Der Transport ist schon heute Streamable HTTP. Die Auth hängt hinter einem
  `Authenticator`-Trait (Bearer jetzt, OAuth 2.1 Resource Server später).

---

## 3. Repo-Struktur

```
brainmatter/
├─ Cargo.toml                 # Workspace
├─ crates/
│  ├─ bm-core/                # keine UI- oder Netz-Abhängigkeiten
│  │  ├─ vault/               # Header, KDF, Key-Wrapping, Recovery-Key, Lock-State
│  │  ├─ store/               # SQLCipher, Migrationen, Repositories, Writer-Actor
│  │  ├─ graph/               # In-Memory-Graph, PageRank, Louvain-Cluster, Pfade
│  │  ├─ search/              # FTS5, Vektor-Index, Hybrid-Ranking (RRF)
│  │  ├─ embed/               # Embedder-Trait, fastembed-Impl, Job-Queue
│  │  ├─ blobs/               # verschlüsselte Anhänge
│  │  ├─ history/             # Revisionen, Change-Sets, Revert
│  │  └─ authz/               # Actor, Scopes, Projekt-Scopes
│  ├─ bm-mcp/                 # rmcp-Server: Tools, Resources, Prompts, Auth-Middleware
│  ├─ bm-macos/               # objc2: LocalAuthentication, Keychain, NSWorkspace-Events
│  └─ bm-cli/                 # Binary `brainmatter-mcp` (stdio↔HTTP-Bridge), später CLI
├─ app/                      # Svelte 5 + SvelteKit 3 (static SPA), Bun
│  └─ src-tauri/              # Tauri-Shell, Wiring, Commands, Tray
├─ spikes/                    # Phase-0-Benchmarks (nicht Teil der App)
├─ scripts/                   # z. B. create-signing-cert.sh
├─ tools/gen-vault/           # Generator für synthetische Test-Vaults (bis 100k Knoten)
└─ docs/
```

Speicherort:
```
~/Library/Application Support/Brainmatter/
├─ vault.header      # KDF-Parameter, Salt, gewrappte Schlüssel. Kein Inhalt im Klartext.
├─ brainmatter.db    # SQLCipher: alles verschlüsselt, auch FTS-Index und Embeddings
├─ blobs/xx/…        # Anhänge (XChaCha20-Poly1305), Name = keyed BLAKE3 (verrät nichts über den Inhalt)
├─ backups/          # verschlüsselte DB-Snapshots (täglich/wöchentlich)
└─ models/           # Embedding-Modell (nicht geheim)
```

---

## 4. Datenmodell

```sql
-- Einträge
nodes(
  id TEXT PRIMARY KEY,          -- UUIDv7 (zeitlich sortierbar, sync-tauglich)
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '', -- Markdown
  kind TEXT NOT NULL DEFAULT 'note', -- note|concept|person|task|source|decision|snippet|…
  props TEXT,                   -- JSON, freie Key-Values (url, status, due …)
  reviewed INTEGER DEFAULT 1,   -- 0 = "neu von KI, noch nicht angesehen"
  rev INTEGER NOT NULL,
  created_at, updated_at, created_by, updated_by,   -- Actor-IDs (Herkunft)
  deleted_at                    -- Soft-Delete / Papierkorb
);

-- Verbindungen
edges(
  id TEXT PRIMARY KEY, src TEXT, dst TEXT,
  relation TEXT NOT NULL DEFAULT 'related_to',
  -- Standard-Vokabular: related_to, part_of, depends_on, example_of, supports,
  -- contradicts, derived_from, mentions (aus [[Wikilinks]]). Frei erweiterbar, normalisiert snake_case.
  weight REAL DEFAULT 1.0, note TEXT,
  origin TEXT,                  -- manual | wikilink | ai | suggestion
  created_at, created_by, deleted_at,
  UNIQUE(src, dst, relation)
);

-- Projekte = verschachtelte Untergraphen
projects(id, parent_id NULL, name, description, color, icon, sort, created_at, created_by, deleted_at);
project_members(project_id, node_id, PRIMARY KEY(project_id, node_id));  -- n:m

-- Layout pro Ansicht, damit der Graph nicht bei jedem Öffnen "springt"
layout_positions(view_id, node_id, x, y, pinned, PRIMARY KEY(view_id, node_id)); -- view_id = Projekt | 'all'

node_tags(node_id, tag);
attachments(id, node_id, filename, mime, size, blob_id, created_at, created_by);

-- Verweise auf lokale Dateien/Ordner (keine Kopie). Das Bookmark überlebt Umbenennen und Verschieben.
file_links(id, node_id, path, bookmark BLOB, label, is_dir, approved_for_ai, last_resolved_at,
           missing, created_at, created_by);
-- Vorlagen/Prompts sind normale Nodes mit kind = 'template'. Sie haben also Suche, Links, Historie
-- und Projekte gratis. props: { slug, arguments: [{name, description, required, default}], scope: global|project }

-- Historie & Undo
change_sets(id, actor_id, client_name, tool, summary, at, reverted_by NULL);  -- 1 MCP-Call = 1 Change-Set
revisions(id, change_set_id, entity_type, entity_id, rev, before BLOB, after BLOB); -- zstd-JSON

-- Intelligenz
embeddings(node_id PRIMARY KEY, model, dim, vec BLOB, content_hash);
link_suggestions(src, dst, score, status, created_at);  -- open | accepted | dismissed

-- Zugriff
actors(id, kind, name, color, token_hash, scopes, project_scope, created_at, last_seen_at);
activity_log(id, actor_id, tool, args_summary, duration_ms, at);  -- ohne Inhalte

-- Volltext (liegt in SQLCipher, also automatisch verschlüsselt)
CREATE VIRTUAL TABLE nodes_fts USING fts5(title, body, tags, content='nodes',
  tokenize="unicode61 remove_diacritics 2");
```

Semantik:
- **Mehrfach-Zugehörigkeit**: Ein Node kann in beliebig vielen Projekten sein. Die Ansicht „Alles“ zeigt den gesamten Graph.
- **Untergraphen**: Im Elterngraph erscheint ein Unterprojekt als Blase (Meta-Knoten, Größe = Anzahl Mitglieder).
  Kanten nach draußen werden zu Bündeln mit Zähler zusammengefasst. Doppelklick zoomt animiert hinein.
- **Rand-Ansicht** (umschaltbar): In einem Projekt werden Nachbarn außerhalb des Projekts blass mit angezeigt.
- **Wikilinks** `[[Titel]]` im Text erzeugen oder entfernen `mentions`-Kanten automatisch. Backlinks gibt es dadurch gratis.

---

## 5. Sicherheit

### Schlüssel-Hierarchie
```
Passwort ──Argon2id(salt, ~0.5 s, 256 MiB)──► KEK ──wrap──┐
Recovery-Key (einmalig angezeigt) ─────────► RK  ──wrap──┼──► DEK (256 bit, zufällig)
Touch-ID-Key (Keychain) ───────────────────► BK  ──wrap──┘        │
                                                        HKDF ├─► SQLCipher-Key (raw)
                                                             └─► Blob-Key (XChaCha20-Poly1305)
```
- Für eine Passwortänderung wird nur der DEK neu gewrappt, die Datenbank muss nicht neu verschlüsselt werden.
- **Recovery-Key** beim Setup (Base32, zum Ausdrucken). Ohne Passwort und Recovery-Key sind die Daten verloren, das ist gewollt.
- Schlüssel liegen in `secrecy`/`zeroize`-Typen. Beim Sperren werden alle DB-Connections geschlossen,
  Graph, Vektoren und Caches verworfen, die Schlüssel gelöscht und der UI-Zustand geleert.

### Touch ID ohne Apple-Developer-Account
- Der BK liegt als Generic-Password-Item in der Login-Keychain, die ACL ist an die Code-Signatur gebunden.
- Ablauf beim Entsperren: Zuerst `LAContext.evaluatePolicy(.deviceOwnerAuthenticationWithBiometrics)`,
  bei Erfolg wird der BK gelesen und der DEK entpackt.
- **Keychain ohne Team ID** (in Spike 4 gemessen): macOS bindet Keychain-Einträge an den **cdhash** des Binaries,
  auch bei stabilem selbstsigniertem Zertifikat. Ein Update kann den alten Eintrag also nicht mehr lesen.
  Deshalb laufen alle Keychain-Zugriffe **ohne UI** (`SecKeychainSetUserInteractionAllowed(false)`). Statt eines Dialogs
  kommt sofort `NotAccessible`. Die App löscht dann den alten Eintrag (das geht ohne Rückfrage), entsperrt einmal per Passwort
  und richtet Touch ID still neu ein, mit neuem zufälligem Account-Namen aus dem Vault-Header.
  UX: **Nach jedem Update einmal das Passwort eingeben**, wie bei vielen Passwort-Managern.
- Das selbstsignierte Zertifikat „Brainmatter Dev“ (`scripts/create-signing-cert.sh`) bleibt trotzdem sinnvoll.
  Es sorgt für stabile TCC-Rechte (Dateizugriff, Mitteilungen) über Builds hinweg.
- Ehrlich gesagt: Das ist ein Software-Gate. Ein Prozess, der als du läuft und auf das Keychain-Item zugreifen darf,
  könnte Touch ID umgehen. Die kryptografische Wurzel bleibt das Passwort. Mit Dev-Account lässt sich später
  auf `kSecAccessControlBiometryCurrentSet` und die Secure Enclave upgraden.

### Lock-Verhalten
- Auto-Lock nach Inaktivität (Standard 60 min) und bei System-Sleep. Optional auch bei Bildschirmsperre.
- Fragt eine KI an, während die App gesperrt ist, wartet der MCP-Request (max. 30 s) auf
  `LockManager::request_unlock(actor)`. Dabei erscheint ein Touch-ID-Prompt „Claude Code möchte auf Brainmatter zugreifen“.
  - Bei Erfolg läuft der Request weiter. Bei Abbruch oder Timeout bekommt die KI den Fehler `vault_locked` mit klarer Meldung.
  - Anti-Spam: Es ist immer nur ein Prompt offen. Nach einer Ablehnung kommt für diesen Actor 5 min lang kein neuer Prompt.
- Nach Fehlversuchen beim Passwort wächst die Wartezeit (Backoff).

### MCP-Absicherung
- Der Server bindet **nur an 127.0.0.1**. Er prüft **Origin- und Host-Header** gegen DNS-Rebinding (Pflicht laut MCP-Spec für lokale Server).
- **Ein Token pro Client** (`bm_…`, 256 bit). Er wird nur einmal angezeigt, gespeichert wird nur der SHA-256-Hash.
- **Scopes** pro Token: `read`, `write`, `delete`. Dazu ein optionaler **Projekt-Scope**,
  z. B. sieht die Arbeits-KI nur das Projekt „Arbeit“.
- Prompt-Injection: Notizinhalte gelten als untrusted Daten und werden in klar abgegrenzten Feldern zurückgegeben.
  Löschen ist immer ein Soft-Delete, jedes Change-Set lässt sich zurücknehmen. Optional verlangt die App vor Löschungen eine Bestätigung im UI.
- Logs und Activity-Log enthalten **nie Notizinhalte**. Es gibt keine Telemetrie.

---

## 6. Graph-Engine, Suche, Intelligenz

- **In-Memory-Graph** (petgraph `StableGraph`). Darauf laufen Nachbarschaft, k-Hop-Subgraph,
  kürzester Pfad, Grad und PageRank („wichtigste Einträge“), Louvain-Cluster (automatische „Themen“) und Waisen-Erkennung.
- **Volltext**: FTS5 mit BM25, Titel stärker gewichtet, Präfix-Suche. Optional eine Trigram-Tabelle für Teilwörter (deutsche Komposita).
- **Embeddings**: **candle + Metal (f32)** mit `multilingual-e5-small` hinter einem `Embedder`-Trait (Entscheidung aus Spike 5, siehe §13).
  Eine Hintergrund-Queue bettet Änderungen nach 2 s Debounce ein. Lange Notizen werden in Chunks zerlegt.
  Die Vektoren liegen verschlüsselt in der DB und werden beim Entsperren als Matrix in den RAM geladen.
  Bis ca. 100k Knoten reicht Brute-Force-SIMD-Cosine (< 10 ms), HNSW (usearch) kommt erst bei Bedarf.
  Das Modell wird lazy geladen und nach längerer Untätigkeit entladen.
- **Hybrid-Suche**: BM25 und Vektor-Ranking werden per Reciprocal Rank Fusion kombiniert, dazu ein Graph-Boost für Nachbarn
  des aktuellen Kontexts. Filter: Projekt, Typ, Tag, Actor, Zeitraum.
- **Auto-Links**: Nach dem Embedding werden die Top-k ähnlichsten, noch nicht verbundenen Knoten über einer Schwelle
  in `link_suggestions` gespeichert.
  - Im UI erscheinen sie als gestrichelte „Geister-Kanten“ und im Inspector als Vorschläge, die man mit einem Klick annimmt oder verwirft.
  - Über MCP liefert `capture` sie direkt mit zurück, zusammen mit einer **Duplikat-Warnung** (Cosine > 0.92).
  - Optional werden Vorschläge über einer hohen Schwelle automatisch angenommen (Standard: aus).

---

## 7. MCP-Schnittstelle (Herzstück)

**Transport**: Streamable HTTP über `rmcp` + axum. Für stdio-Clients gibt es die Bridge `brainmatter-mcp`. Sie liest den Token aus der Keychain
(`--client claude-desktop`) und startet die App bei Bedarf (`open -b app.brainmatter`).

**Design-Regeln für KI-Ergonomie**
- Wenige, mächtige Tools statt CRUD-Wildwuchs. Batch-fähig, damit die KI weniger Roundtrips braucht.
- Jedes `ref` akzeptiert eine UUID, einen kurzen ID-Präfix oder einen **exakten Titel**. Ist er mehrdeutig, kommt ein Fehler mit Kandidatenliste.
- Ausgaben sind **kompaktes Markdown** (token-sparsam) plus `structuredContent` (JSON mit `outputSchema`).
- Limits und Cursor sind überall vorhanden, damit Antworten nie explodieren. Fehler sind handlungsleitend („Meintest du …?“).
- Jeder schreibende Call wird ein Change-Set mit Actor und Client-Name, ist im UI live sichtbar und lässt sich mit einem Klick zurücknehmen.

**Tools (v1)**

| Tool | Zweck |
|---|---|
| `search {query, project?, kinds?, tags?, mode=hybrid\|text\|semantic, limit}` | Treffer mit Snippet, Projekten und Top-Relationen |
| `get {ref, include?: [links, backlinks, history, attachments]}` | Vollständiger Eintrag + Nachbarschaft |
| `explore {ref, depth=2, relations?, limit}` | Subgraph als kompakte Adjazenzliste (`A —depends_on→ B`) |
| `capture {nodes:[{key?, title, body, kind?, tags?, projects?}], links:[{from, to, relation, note?}], project?}` | **Haupt-Schreibtool**: atomares Batch-Anlegen, `from`/`to` dürfen lokale Keys oder bestehende Refs sein. Antwort: IDs, Duplikat-Warnungen, Link-Vorschläge |
| `update {ref, title?, body?, body_mode=replace\|append\|prepend, tags_add?, tags_remove?, kind?, props?}` | Ändern (Append, damit nichts überschrieben wird) |
| `link {from, to, relation, note?}` · `unlink {from, to, relation?}` | Verbindungen |
| `list_projects {}` · `create_project {name, parent?, description?}` | Projektbaum mit Zählern |
| `add_to_project {refs[], project}` · `remove_from_project {…}` | Mitgliedschaft |
| `project_overview {project}` | Beschreibung, Statistik, wichtigste Knoten (PageRank), Themen-Cluster, letzte Änderungen, offene Tasks. Gedacht als Kontext-Laden zu Sessionbeginn |
| `recent_changes {since?, project?, actor?}` | Was hat sich geändert (auch durch andere KIs) |
| `find_path {from, to, max_depth}` | Wie hängen zwei Themen zusammen? |
| `merge {keep, merge[]}` | Duplikate zusammenführen (Kanten werden umgehängt) |
| `delete {ref}` | Soft-Delete in den Papierkorb (Scope `delete`) |
| `revert {change_set}` | Eigenes Change-Set zurücknehmen |

| `list_templates {project?, query?}` · `get_template {ref, args?}` | Vorlagen finden und gerendert laden (§7a) |
| `save_template {title, body, arguments?, projects?}` | Vorlage anlegen oder aktualisieren |
| `link_file {node, path, label?}` · `read_linked_file {link, max_bytes?}` | Datei-Verknüpfungen (§7b) |

**Resources**: `brainmatter://node/{id}`, `brainmatter://project/{id}` (Übersicht als Markdown) und `brainmatter://template/{slug}`, abonnierbar.
**Prompts**: die eingebauten `recall` (Projektkontext laden) und `capture_session` (Erkenntnisse der Session in den Graph schreiben)
**sowie jede Vorlage aus der Bibliothek** (§7a).
**Server-Instructions** (werden beim `initialize` mitgeschickt): „Erst suchen, dann anlegen. Bestehende Einträge
lieber erweitern. Ein Gedanke pro Eintrag. Großzügig verlinken, mit dem Relations-Vokabular X. Immer einem Projekt zuordnen.“

**Onboarding im UI**: Die Seite „KI verbinden“ erzeugt Token und fertige Snippets:
- Claude Code: `claude mcp add --transport http --scope user brainmatter http://127.0.0.1:47474/mcp --header "Authorization: Bearer …"`
- Claude Desktop, Codex: JSON/TOML mit dem Pfad zu `brainmatter-mcp`
- Cursor: `mcp.json` mit URL + Header

Getestet wird manuell mit dem MCP Inspector und automatisiert mit Snapshot-Tests (insta) der Tool-Ausgaben.

### 7a. Vorlagen- & Prompt-Bibliothek *(Erweiterung nach dem Hauptplan)*
- Eine Vorlage ist ein Markdown-Eintrag (`kind = template`) mit `{{platzhaltern}}` und optionalen Argument-Definitionen.
  Beispiele: „Code-Review-Checkliste“, „Projekt-Kickoff“, „Bug-Report“, „Meeting-Notiz“, „System-Prompt für Projekt X“.
- **Geltungsbereich**: Globale Vorlagen sind überall sichtbar. Projekt-Vorlagen sieht nur, wer Zugriff auf das Projekt hat,
  ein Projekt-Scope am Token wirkt also auch hier. Unterprojekte erben die Vorlagen ihrer Elternprojekte.
- **Über MCP-Prompts**: Jede sichtbare Vorlage erscheint in `prompts/list`, mit Name = Slug und Arguments = Platzhaltern.
  `prompts/get` rendert sie. In Claude Code wird daraus z. B. `/mcp__brainmatter__code-review`.
  Ändert sich die Bibliothek, geht `notifications/prompts/list_changed` raus.
- **Über Tools** für Clients ohne Prompt-Support oder für eine KI, die selbst entscheidet, wann sie eine Vorlage braucht:
  `list_templates` und `get_template`. `capture` kann einen Eintrag direkt aus einer Vorlage erzeugen (`from_template`).
- **UI**: Abschnitt „Bibliothek“ in der Sidebar mit Editor, Live-Vorschau der Platzhalter, „Mit Werten kopieren“
  und Verwendungsstatistik (welche KI hat die Vorlage wann geladen).

### 7b. Verknüpfungen zu lokalen Dateien *(Erweiterung nach dem Hauptplan)*
- In der UI zieht man Dateien oder Ordner aus dem Finder auf einen Eintrag, in Markdown schreibt man `[Spec](file:///…)`,
  über MCP nutzt man `link_file`. Gespeichert werden Pfad und **macOS-Bookmark**
  (`NSURL bookmarkData`). Wurde die Datei verschoben, wird das Bookmark aufgelöst und der Pfad aktualisiert.
  Ist sie gelöscht, wird der Link als „fehlt“ markiert.
- Im UI zeigt der Link-Chip Icon, Name und Pfad. Es gibt Quick Look (Space), „Im Finder zeigen“ und Öffnen mit der Standard-App.
- **Für KIs**: `get` liefert die Pfade mit. Claude Code und andere lokale Agenten können die Datei dann mit ihren eigenen Tools lesen.
  Für Clients ohne Dateizugriff gibt es `read_linked_file` (nur Text, Größenlimit, Scope `files:read`).
- **Sicherheit**: `read_linked_file` liest **nur verknüpfte Dateien**, niemals beliebige Pfade. Wenn eine KI selbst einen Link anlegt,
  ist dieser erst nach deiner Bestätigung im UI für `read_linked_file` freigegeben (`approved_for_ai`). Sonst könnte eine KI
  per `link_file` + `read_linked_file` beliebige Dateien lesen. Die Datei-Inhalte selbst landen nie in der DB, nur der Verweis.

### 7c. Projekt-Dokumentation *(Erweiterung nach dem Hauptplan)*
- **Handbuch pro Projekt**: Doku-Seiten (`kind = doc`) als Baum mit Kapiteln, Unterseiten und manueller Reihenfolge
  (eigene Tabelle `doc_pages(node_id, project_id, parent_id, sort, source_path, source_hash)`).
- **Lese-Ansicht**: Typografie wie ein Buch, Inhaltsverzeichnis aus den Überschriften, Vor/Zurück und Suche innerhalb der Doku.
  Zum Bearbeiten wechselt man in den Editor.
- **Zwei Quellen**:
  1. In Brainmatter geschriebene Seiten (von dir oder der KI).
  2. **Verknüpfte Doku-Ordner** (z. B. `docs/` oder `README.md` eines Repos). Sie werden schreibgeschützt gespiegelt, indexiert und per FSEvents aktuell gehalten.
     Die Quelle bleibt die Datei. Brainmatter speichert verschlüsselt eine Kopie für Suche und Embeddings.
- **Für KIs**: `list_docs {project}` liefert das Inhaltsverzeichnis mit Abschnitts-IDs. `read_doc {ref, section?}` liefert **nur den gewünschten Abschnitt**
  und spart so Tokens. Semantische Suche läuft auf Abschnittsebene. `write_doc` legt Seiten an oder ändert sie (nicht bei gespiegelten Quellen).

---

## 8. UI/UX

**Screens**
1. **Onboarding**: Passwort (mit Stärkeanzeige), Recovery-Key anzeigen und bestätigen, Touch ID aktivieren, Modell laden, erste KI verbinden.
2. **Lock-Screen**: Touch-ID-Button und Passwortfeld, sonst nichts.
3. **Hauptfenster** (3 Spalten, macOS-Vibrancy, Hell/Dunkel automatisch):
   - *Links*: Projektbaum (verschachtelt, Drag & Drop), Favoriten, Tags, gespeicherte Ansichten, „Ungeprüft (von KI)“, Papierkorb.
   - *Mitte*: Graph, Notiz oder Split. Oben eine Breadcrumb `Alles › Projekt › Unterprojekt`, Filter-Chips und eine Live-Anzeige für KI-Aktivität.
   - *Rechts*: Inspector mit Inhalt, Verbindungen (gruppiert nach Relation, ein- und ausgehend), Vorschläge, Projekte, Anhänge,
     Historie (Timeline mit Actor-Farben) und Herkunft.
4. **⌘K Command-Palette**: Fuzzy-Suche über Einträge, Projekte und Aktionen.
5. **KI-Aktivität**: Live-Feed, z. B. „Claude Code: 4 Einträge + 6 Verbindungen in *Brainmatter*“, mit [Anzeigen] und [Rückgängig].
6. **Einstellungen**: Sicherheit, KI-Clients (Tokens, Scopes, Projekt-Scopes, zuletzt gesehen), Suche/Modell, Backups, Darstellung.
7. **Quick Capture**: Globaler Hotkey (⌥⇧Space) öffnet ein Mini-Fenster wie Spotlight.

**Graph-Interaktion** (Sigma.js v3 + graphology, WebGL)
- Hover hebt die Nachbarschaft hervor, Klick wählt aus und öffnet den Inspector, Doppelklick öffnet die Notiz bzw. **taucht in das Unterprojekt ein**. Esc geht eine Ebene hoch.
- Lokaler Graph: Tiefe 1–3 um den ausgewählten Knoten.
- Einfärben nach Projekt, Typ oder automatischem Themen-Cluster. Größe nach Grad oder PageRank.
- Filter nach Typ, Tag, Relation und Actor (z. B. „nur was Claude diese Woche angelegt hat“). Ein **Zeitregler** spielt das Wachstum des Graphen ab.
- Layout: ForceAtlas2 im Web Worker. Positionen werden **pro Ansicht gespeichert**, damit die mentale Karte stabil bleibt. Knoten lassen sich per Drag pinnen.
- Neue Knoten von KIs blenden mit einem Puls ein. Eine neue Verbindung zieht man mit ⌥-Drag von Knoten zu Knoten und wählt dann die Relation.
- Große Graphen: Daten kommen binär (Float32Array) über einen Tauri-Channel statt als JSON, mit Level of Detail (Labels nur bei Zoom).

**Editor**: CodeMirror 6 mit Markdown-Live-Preview (ähnlich Obsidian). Markdown bleibt die Quelle, weil KIs Markdown schreiben.
`[[`-Autocomplete für Wikilinks, `#tag`-Autocomplete, Bilder einfügen erzeugt einen Anhang.

**Tastatur**: ⌘N neu · ⌘K Palette · ⌘L verlinken · ⌘[ / ⌘] zurück/vor · J/K/H/L durch Nachbarn springen · Space Quick-Look.

**Komponenten**: Tailwind 4, shadcn-svelte (bits-ui), Lucide-Icons, SF Pro.

---

## 9. Performance-Ziele

| Metrik | Ziel |
|---|---|
| Start bis Lock-Screen | < 300 ms |
| Entsperren + Laden (50k Knoten) | < 1.5 s (davon ~0.5 s bewusst Argon2) |
| Hybrid-Suche p95 (50k Knoten) | < 25 ms |
| MCP-Tool-Roundtrip p95 | < 15 ms (Embedding läuft asynchron) |
| Graph-Rendering | 50k Knoten / 150k Kanten mit 60 fps, 10k mit Live-Layout |
| RAM im Leerlauf | < 200 MB (Modell lazy) |
| Embedding pro Notiz | ~5–20 ms (M-Chip, Hintergrund) |

So werden sie erreicht: WAL, Prepared Statements, Writer-Actor, In-Memory-Graph und Vektormatrix, binäres IPC,
virtualisierte Listen, criterion-Benchmarks und der Testvault-Generator (100k Knoten) als Regressionstest.

---

## 10. Tech-Stack

**Rust**
- Shell: `tauri` 2 (+ Plugins: single-instance, window-state, autostart, global-shortcut), `tauri-specta`/`specta`
- Async/Netz: `tokio`, `axum`, `tower-http`
- MCP: `rmcp` (offizielles SDK; Features server, macros, transport-streamable-http-server)
- DB: `rusqlite` (bundled SQLCipher) + `rusqlite_migration`
- Krypto: `argon2`, `chacha20poly1305`, `hkdf`, `sha2`, `blake3`, `rand`, `zeroize`, `secrecy`
- macOS: `objc2`, `objc2-local-authentication`, `objc2-app-kit`, `security-framework`
- Graph/Suche: `petgraph`, `candle-core`/`candle-transformers` (Metal), `tokenizers`
- Sonstiges: `pulldown-cmark` (Wikilinks/Tags parsen), `uuid` (v7), `serde`, `schemars`, `zstd`, `thiserror`, `anyhow`, `tracing`
- Tests: `criterion`, `proptest`, `insta`

**Frontend**
- Svelte 5 + SvelteKit 3 (adapter-static; Config in `vite.config.ts`, `#lib`-Imports), TypeScript 6 (TS 7 wird von SvelteKit noch nicht unterstützt), Vite 8, Bun. Node ≥ 22.17
- `sigma` v4, `graphology`, `graphology-layout-forceatlas2` (Worker)
- CodeMirror 6 (`@codemirror/lang-markdown`)
- Tailwind 4, shadcn-svelte
- Playwright gegen gemocktes IPC (tauri-driver unterstützt macOS nicht)

---

## 11. Roadmap

**Phase 0: Spikes (Risiken klären, bevor gebaut wird)**
1. Tauri 2 + SQLCipher bauen auf arm64. FTS5 innerhalb von SQLCipher, Build-Zeit, OpenSSL vs. CommonCrypto.
2. rmcp-HTTP-Server im Tauri-Prozess: Claude Code verbindet sich und ruft ein Tool auf. Die stdio-Bridge funktioniert mit Claude Desktop.
3. Sigma.js in WKWebView: Läuft der 50k/150k-Testgraph mit 60 fps? Hält der FA2-Worker mit?
4. Touch ID (objc2) + Keychain-Item mit selbstsigniertem Zertifikat: übersteht die ACL einen Rebuild?
5. fastembed + `multilingual-e5-small`: Latenz, RAM, ONNX-Runtime im .app-Bundle.

*Fertig, wenn* alle fünf grün sind oder eine Alternative entschieden ist.

**Phase 1: Kern & Tresor** (`bm-core`)
- Vault (Setup, Entsperren, Sperren, Recovery-Key, Passwort ändern), Migrationen
- CRUD für Nodes, Edges, Projekte, Mitgliedschaften und Tags
- Writer-Actor, EventBus, Revisionen und Change-Sets mit Revert
- FTS5, Wikilink-Parsing, In-Memory-Graph

*Fertig, wenn* Integrationstests (anlegen, suchen, verlinken, revert) auf dem verschlüsselten Vault grün sind,
die DB-Datei ohne Schlüssel nicht lesbar ist und Manipulation erkannt wird.

**Phase 2: Desktop-App MVP**
- Tauri-Shell mit Menüleisten-Modus, Onboarding und Lock-Screen (Passwort)
- Hauptlayout, Projektbaum, Graph (persistente Positionen, Untergraph-Navigation, Breadcrumb)
- Editor, Inspector, ⌘K, Live-Updates, Auto-Lock

*Fertig, wenn* sich eigenes Wissen komplett per UI pflegen lässt und der 10k-Testvault flüssig läuft.

**Phase 3: MCP-Hub** (`bm-mcp`, `bm-cli`)
- Alle Tools, Resources, Prompts und Server-Instructions
- Token-Verwaltung mit Scopes und Projekt-Scopes, Origin/Host-Prüfung, Connect-Snippets, stdio-Bridge
- Activity-Feed mit Revert, Herkunfts-Badges, Filter „Ungeprüft“, Lock → Touch-ID-Prompt

*Fertig, wenn* Claude Code, Claude Desktop und Cursor lesen und schreiben, KI-Änderungen live erscheinen, sich zurücknehmen lassen
und eine Test-Session („Erfasse das Wissen aus Repo X als Projekt“) einen sauberen Graph ergibt.

**Phase 4: Intelligenz**
- Embedding-Pipeline, Hybrid-Suche, Link-Vorschläge in UI und MCP, Duplikat-Warnung, `merge`
- `project_overview` mit PageRank und Clustern, `find_path`, Cluster-Färbung, Waisen-Ansicht

*Fertig, wenn* die Suche nach „tokio Nebenläufigkeit“ den Eintrag „Async Runtime“ findet und `capture` vor Duplikaten warnt.

**Phase 5: Anhänge, Touch ID, Feinschliff**
- Verschlüsselte Anhänge (Drag & Drop, Bilder einfügen, PDF-Vorschau), Touch ID für UI und KI-Anfragen
- Automatische Backups, Quick Capture, Zeitregler, Theming
- Performance-Tuning bis zu den Zielwerten, .app-Bundle + Signierung + DMG

*Fertig, wenn* die Performance-Ziele auf dem 50k-Vault erreicht sind und die App per DMG installierbar ist. **= v1.0**

**Erweiterungen nach dem Hauptplan** (Wünsche vom 2026-10-29; erst wenn v1.0 steht)
1. **Vorlagen- & Prompt-Bibliothek** (§7a): Vorlagen werden zu MCP-Prompts und Tools, dazu der UI-Abschnitt „Bibliothek“.
2. **Datei-Verknüpfungen** (§7b): Bookmarks, Drag & Drop aus dem Finder, `read_linked_file` mit Freigabe.
3. **Projekt-Dokumentation** (§7c): ein Handbuch pro Projekt mit Lese-Ansicht, gespiegelten Doku-Ordnern und abschnittsweisem Laden für KIs.

**Danach (v1.x+)**
Obsidian/Markdown-Import und -Export, Cloud-KIs (OAuth 2.1 + Tailscale Funnel oder Cloudflare Tunnel), mehrere Vaults (Privat/Arbeit),
Sync zwischen Geräten (Op-Log und UUIDv7 sind schon vorbereitet, CRDT z. B. Loro), iOS-Begleiter, Textextraktion aus PDFs,
Secure-Enclave-Touch-ID mit Dev-Account.

---

## 12. Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| SQLCipher-Build (OpenSSL) zäh | Spike 1. Bundled-Variante, auf macOS evtl. CommonCrypto |
| Touch ID ohne Dev-Account nur ein Software-Gate | Passwort bleibt die Krypto-Wurzel, Upgrade-Pfad ist dokumentiert |
| Keychain-ACL bricht bei Rebuilds | Selbstsigniertes, stabiles Signing-Zertifikat |
| WebGL in WKWebView zu langsam bei Riesengraphen | LOD, Projekt-Blasen statt aller Knoten, Fallback cosmos.gl |
| Embedding-RAM (~0.9 GB geladen) | Lazy laden, nach 10 min Inaktivität entladen. Beim Entsperren direkt einbetten, im Hintergrund drosseln |
| KIs müllen den Graph zu | Server-Instructions, Duplikat-Warnung, Flag „Ungeprüft“, Revert pro Change-Set |
| Passwort vergessen | Recovery-Key beim Setup ist Pflicht |
| DB-Korruption | WAL, `integrity_check` beim Start, tägliche verschlüsselte Backups |
| MCP-Spec ändert sich | rmcp folgt der Spec, die Tool-Schicht bleibt dünn über dem Kern |

---

## 13. Ergebnisse Phase 0 (2026-10-29, M-Mac, macOS 27.0.1, Rust 1.99.1)

| Spike | Ergebnis | Entscheidung |
|---|---|---|
| 1 · SQLCipher + FTS5 | ✅ DB, WAL und FTS-Index sind verschlüsselt (keine Klartext-Bytes auf der Platte), ein falscher Schlüssel wird abgelehnt. Umlaute werden gefaltet („nebenlaufigkeit“ findet „Nebenläufigkeit“), Präfix- und Tag-Suche funktionieren, Soft-Delete wird aus dem Index entfernt. Auf macOS nutzt SQLCipher **CommonCrypto** (`Security.framework`), OpenSSL ist nicht nötig. Build ~1,5 min kalt | `rusqlite` + `bundled-sqlcipher`, Test `crates/bm-core/tests/spike_sqlcipher.rs` |
| 2 · MCP über HTTP | ✅ `rmcp` 3.8 Streamable HTTP mit Bearer-Auth (401) und Ablehnung von Browser-Origins (403). Tool-Roundtrip **p50 0,3 ms / p95 0,5 ms**. **Echter Claude-Code-Client verbunden** und `ping` aufgerufen | rmcp prüft Host und Origin schon selbst (`enforce_origin_validation`). Die stdio-Bridge kommt in Phase 3 |
| 3 · Graph-Rendering | ✅ Sigma 4 in WKWebView: 10k mit Live-FA2 bei 60 fps; **50k/150k bei 59 fps (p95 18 ms)**; 100k/300k bei 51 fps. 60 fps ist die Obergrenze des Displays | Ziel erreicht. Offen: Messung auf Retina (dpr 2) und das kalte erste Rendern (2,8 s, Shader) |
| 4 · Touch ID + Keychain | ✅ Touch ID wird erkannt, der **Finger-Prompt funktioniert**, Keychain-Round-Trip ok. Das Zertifikat „Brainmatter Dev“ ist angelegt. ⚠️ Die Keychain-ACL hängt am **cdhash**, ein Rebuild kann alte Einträge also nicht mehr lesen. Mit abgeschalteter Keychain-UI kommt statt eines Dialogs nach 0,6 s ein Fehler, und das Löschen geht ohne Rückfrage | `objc2-local-authentication` + `security-framework`. Nach jedem Update einmal das Passwort, dann wird Touch ID still neu eingerichtet (§5) |
| 5 · Embeddings | ONNX Runtime (fastembed): e5-small bei 1,8 GB RSS (Spitze 3,7 GB), 15 ms pro Query, braucht einen `clang_rt`-Link-Workaround. EmbeddingGemma: beste Qualität (13/13), aber 80 ms und >10 GB. **candle + Metal f32 mit e5-small: 0,3 s Laden, 4,7 ms pro Query, 1,4 ms pro Notiz, ~0,9 GB Spitze, 11/13 korrekt (auch DE↔EN)**. f16 zerstört die Qualität (1/13) | **candle + Metal, f32, `multilingual-e5-small`**. Lazy laden und nach Inaktivität entladen |

Nebenbei aufgefallen: `tauri` 3 ist noch Alpha, also bleibt es bei Tauri 2. SvelteKit 3 hat `svelte.config.js` abgeschafft und `$lib` durch `#lib` ersetzt.
TypeScript 7 ist mit SvelteKit noch inkompatibel.

## 14. Stand Phase 1: Kern & Tresor (2026-10-29)

**Fertig** (`crates/bm-core`, 21 Tests grün):
- **Tresor**: Header mit Slots für Passwort (Argon2id), Recovery-Key (36 Zeichen Crockford-Base32 mit Prüfsumme) und Keychain.
  Der DEK wird per XChaCha20-Poly1305 gewrappt, die AAD bindet jeden Slot an Vault und Slot-Typ. Subkeys entstehen per HKDF.
  Der Header wird atomar geschrieben (0600). Passwortwechsel, Recovery-Rotation und Touch-ID-Enrollment sind fertig. Manipulation führt zur Ablehnung.
- **Engine**: ein Writer (IMMEDIATE-Transaktion pro Aktion) plus Read-Pool (WAL), Events per `broadcast` nach dem Commit.
  - CRUD für Einträge, Kanten, Projekte und Mitgliedschaften
  - `capture` legt einen Subgraph atomar an, mit lokalen Keys und Duplikat-Hinweisen
  - Wikilinks erzeugen und entfernen `mentions`-Kanten, inklusive Backlinks auf später angelegte Titel (`wikilink_refs`, Migration V2)
  - Referenzen über Titel, Kurz-ID (`#a1b2c3d4`) oder UUID, mit Kandidaten bei Mehrdeutigkeit und Vorschlägen bei „nicht gefunden“
- **Historie**: Jede Aktion ist ein Change-Set mit Snapshots. Revert läuft als neues Change-Set, auch Revert-of-Revert (Redo).
  Die Konfliktprüfung vergleicht Inhalte. Einträge, die eine KI anlegt oder ändert, landen als „ungeprüft“.
- **Suche**: FTS5 mit gewichtetem `rank` (Migration V3), zweistufig (Top-N, dann Snippets), Stoppwörter DE/EN,
  Filter für Projekt-Teilbaum, Typ und Tag per korreliertem `EXISTS`.

**Benchmark** (`cargo run --release -p bm-core --example bench_engine`, 50k Einträge, Zipf-Korpus mit 60 Wörtern pro Notiz):

| Operation | Ergebnis | Ziel |
|---|---|---|
| Insert über `capture` | ~3.000 Einträge/s (inkl. 2 Links pro Eintrag, Revisionen, FTS) | – |
| Suche | p50 7 ms, **p95 13 ms** | < 25 ms ✅ |
| Suche + Projekt + Tag | p50 10 ms, **p95 24 ms** | < 25 ms ✅ |
| Worst Case: 2 Begriffe, die in jedem Eintrag vorkommen | ~105 ms | ⚠️ Dynamisches Ausfiltern über die Dokumentfrequenz in Phase 4 |
| `node_detail` / Titel auflösen / Update | 80 µs / 16 µs / 0,85 ms (p95) | ✅ |
| DB-Größe | 294 MB | Revisionen später mit zstd komprimieren (Format-Byte ist schon vorgesehen) |

**Offen für Phase 1**: In-Memory-Graph (petgraph) für `explore` und `find_path`. Er kommt zusammen mit den MCP-Tools in Phase 3,
bis dahin reichen die indizierten SQL-Nachbarschaftsabfragen (80 µs). Unicode-Groß/Klein beim Titelvergleich (SQLite `NOCASE` faltet nur ASCII).
