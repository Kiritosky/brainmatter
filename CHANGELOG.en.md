# What's new

This text appears in the app when it runs in English, and above the German version in the release notes on GitHub. Every version in [CHANGELOG.md](CHANGELOG.md) needs its entry here too.

## 1.4.0 · 2026-11-13

When an AI client needs Brainmatter, the main window stays closed. You type the password into a small window.

### New
- **The password for AI clients, in a small window:** When an AI client finds Brainmatter locked, Brainmatter asks for your master password in a small window and names the client that wants access. This happens when Touch ID is not set up, or when you choose “Use Password” at Touch ID. The client then carries on right away. “Not now” turns it down at once, and it does not ask again for a while.
- **In the centre or in the menu bar:** Under Settings → Locking, choose where the window appears: in the centre of the screen, or as the menu bar panel under its icon.
- **Started by an AI client, only in the menu bar:** When Brainmatter is not running and an AI client starts it, it stays in the menu bar, without a window and without a Dock icon.

### Fixed
- After locking, Touch ID could ask even while the main window was closed. Now it asks once you open the window.
- An AI connection set up with a different Brainmatter address, such as one for a development build, brought the main window to the front when nothing answered there.

## 1.3.0 · 2026-11-05

Your commands and skills now also show up in the skill menu of your AI tools, and the memory rule keeps itself up to date.

### New
- **In the skill menu:** For Claude Code, Codex and Cursor, Brainmatter can put its commands, your reviewed templates and your skills into the app's skill folder, as far as the connection may see them: `~/.claude/skills`, `~/.agents/skills` or `~/.cursor/skills`. That way you can pick them in apps that show skills but not MCP commands. Brainmatter keeps the folder up to date with every change and touches only what it put there itself. New connections have it on; existing ones get a switch under Settings → AI connections.
- **Directly as files, or through Brainmatter:** For each skill you choose how it reaches the skill folder: whole, with all its files, or as a short pointer that loads it from the vault only when it is used. Skills from GitHub go as files, your own through Brainmatter. Either way it is stored there unencrypted; as a pointer, only its name and description.
- **Commands through `get_template`:** `get_template` also returns the built-in commands filled in, the same way the command list shows them.
- **The memory rule stays current:** When the app starts or the vault is unlocked, a rule Brainmatter wrote earlier gets the current wording, where it stands and in its read-only or writing variant.

## 1.2.0 · 2026-11-04

Your AI tools now turn to Brainmatter on their own, and a skill vault keeps the instructions they work with in one place.

### New
- **Brainmatter as memory:** When you connect an AI client, Brainmatter can add a short rule to its instructions: `~/.claude/CLAUDE.md` for Claude Code, `~/.codex/AGENTS.md` for Codex, `~/.cursor/rules` for Cursor. It then looks things up before every task and saves decisions, findings and your corrections without being asked. For Claude Desktop and any other AI client with MCP, you get the rule to copy. Existing connections get a switch for it under Settings → AI connections.
- **Skill vault:** Skills are instructions an AI client loads by itself when a task fits, such as how you want commit messages or how to fill in a PDF form. Keep them per project or for everything, write and edit them in the Library, add files such as scripts, and every connected AI client can use them, whichever it is. Skills you have reviewed also appear as commands.
- **Skills from GitHub:** Paste the address of a public repository or folder, choose which skills to take, and they are in the vault. Later, one click brings a skill up to date with its source. Repositories in the common Agent Skills layout work as they are, and an export writes skills back in that layout.
- **Install with Homebrew:** `brew install --cask kiritosky/tap/brainmatter`. The app keeps updating itself as before.
- **Clearer hints for every AI client:** The MCP server tells every connected AI client more precisely when to look things up and when to save.

### Improved
- Clearer English throughout the interface.

### Fixed
- Search by meaning no longer depends on a ZIP library with known vulnerabilities.
- Errors while downloading the model for search by meaning were always in German.
- An empty entry or page asked to be written to in German only.
- The in-browser demo's error messages now follow the interface language.

## 1.1.0 · 2026-11-04

Brainmatter now speaks English, brings in your notes from other apps, and can keep its backups in a second place.

### New
- **English:** The app follows the language of macOS. If German comes first in System Settings, everything stays German; otherwise it is English.
- **Import Markdown:** A folder from Obsidian, Logseq or a Brainmatter export becomes entries. Folders become projects, front matter becomes tags and kind, links between the files become links. You see what is coming first, and the whole import can be undone with one click. Find it under Settings → Backups.
- **Backups in a second place:** Every backup can also go to an external drive or another folder. If the drive is not there, Brainmatter catches up as soon as it is plugged in again.
- **Carry on on a new Mac:** During setup, choose “Restore from a backup”, point to the folder and open it with the master password you used then.
- **Docs follow their files live:** Mirrored docs folders update as soon as you save there, not only when you open the docs.
- **Brainmatter in the menu bar:** A click on the icon shows whether the vault is open, what the AIs changed last, and a search. You can unlock right there, with Touch ID or the password. A right click opens the menu as before.
- **Only in the menu bar while the window is closed:** Close the window and Brainmatter leaves the Dock and ⌘Tab, and keeps running up top.

### Improved
- **Large vaults:** Unlocking and searching are faster at 50,000 entries, and a change loads only what it touched instead of the whole graph.
- Large graphs show their entries first and their links a moment later.

### Fixed
- Links and references told upper and lower case apart for umlauts, such as “Ärger” and “ärger”.

## 1.0.0 · 2026-11-02

Writing, templates and getting started are redone, the graph stays smooth at 50,000 entries, and the app looks calmer.

### New
- **Write docs right on the page:** Type `/` for headings, lists, tables and every other format, even if you do not know the Markdown for them. ⌘B and ⌘I format the selection.
- **Tour:** On first start, a short tour shows the places that matter. Restart it any time from ⌘K.
- **Help in the settings:** What the app can do, and which MCP tools and commands an AI client can use.
- **Templates in three steps:** A new template is made in a guided flow, and a template becomes an entry with one click.
- **Search while assigning:** Wherever you choose a project, you can now search for it or create it on the spot, in Quick Capture too with ⌘P.
- **Export:** All entries and pages as Markdown files, in folders by project.
- **Saved views:** Keep filters and colouring under a name and bring them back with one click.
- **Faster in the graph:** Drag from one entry to another while holding ⌥ to link them, double-click to open the project underneath, Esc goes up a level.
- **For AI models:** Projects and documentation pages can be read as MCP resources, and a connected model learns when its templates change.

### Improved
- **Large graph:** At 50,000 entries and about 150,000 links, zooming, panning and hovering are now smooth. Before, the app stalled for up to several seconds at each of these.
- **Faster start:** At that size, the graph shows up much sooner after unlocking and needs less work afterwards.
- **Search by meaning:** Far fewer wrong hits when nothing fits your query.
- **Faster search:** Very short search words that match many entries are up to seven times faster in large vaults.
- **Calmer look:** Many small things reworked, from shadows and icons to the selection colour.

### Fixed
- While writing docs, the cursor could jump and just-typed text could get lost.
- A full sidebar pushed the footer and the graph tools out of view in small windows.
- Entries moved a little every time a view opened. They now keep their places.
- Quick Capture showed no projects after unlocking.
- If another app uses the Quick Capture shortcut, the settings now say so.

## 0.7.0 · 2026-11-01

You can now give models commands for Brainmatter, and the app adapts more to you.

### New
- **Commands for AI models:** `setup-project` builds structure and docs for a project, `update-docs` brings the docs up to date, `capture-session` keeps what a session learned, `recall` answers from Brainmatter and `tidy-project` tidies a project. In Claude Code, for example, as `/mcp__brainmatter__setup-project`.
- **Shared memory:** Models are now told to save decisions, insights and your corrections on their own, so the next model can build on them. What fits no project goes into the `AI Memory` project.
- **New hover behaviour:** The entry under the pointer grows, and the ones next to it move aside. The big white ring is gone.
- **Graph settings:** Project backgrounds and names, making room and dimming on hover, entry size, line weight and how many labels show.
- **More under General:** Accent colour, reduce motion, show or hide AI activity, turn off what's new after updates.

## 0.6.0 · 2026-11-01

Your projects are now clearly apart in the graph. Lines between them appear only where entries really are linked.

### New
- **Projects as groups:** Each project gets its own circle with its name in the graph, with subprojects inside. Click the name to open the project.
- **Links across projects:** Links between two projects stay visible as lines and do not pull the circles into each other. Linked projects sit closer together.
- **Hiding without gaps:** When you hide projects, the view closes in on what is left.

### Changed
- On first opening, Brainmatter lays out every view once more. Positions moved by hand are lost this one time.
- Moving a project also lays out the view again.
- Very large graphs take a while for their first layout: about a minute at 50,000 entries. The app stays usable meanwhile.

## 0.5.1 · 2026-11-01

You now see live when an AI client writes something in Brainmatter.

### New
- **Live AI activity:** Above the graph, a short notice shows which AI client changed what, and the entries concerned pulse. This is for writing only: reading leaves no trace.

### Fixed
- The list of changes in the app shows all versions again. In 0.5.0 older entries were missing.

## 0.5.0 · 2026-11-01

You keep the overview as the graph grows: projects can be hidden, the graph can be filtered, and AI clients need noticeably fewer tokens.

### New
- **Hide projects:** Hover over a project and click the eye, or choose `…` → “Show only this one”. Hidden projects disappear from the sidebar and the graph. An entry that is also in a visible project stays in view. Search still finds everything, and AI clients are not affected.
- **Graph filters:** Filter by project, kind, tag and by who wrote something.
- **Local graph:** Shows only what lies within up to three links of the selection.
- **Favourites:** The star in an entry puts it at the top of the sidebar.
- **Tags in the sidebar:** One click filters the graph.
- **Tags for docs:** Every page can carry tags, mirrored ones too. The docs can be filtered by them.
- **⌘K finds more:** Projects too, and actions such as “What’s new”.

### For AI clients
- **Fewer tokens:** Tool descriptions and instructions cost about a third less per session, around 3,200 instead of 4,800 tokens. A test makes sure it stays that way.
- `write_doc` takes tags, `list_docs` shows them and filters with `tag`.
- `recent_changes` returns 10 changes instead of 15 by default.

## 0.4.0 · 2026-11-01

Projects can now be managed properly, and the app tells you what changed.

### New
- **Rename projects:** Double-click the name in the sidebar, or use the `…` menu next to the project.
- **Move projects:** Drag a project onto another to put it inside. Dragged to its top or bottom edge, it lands next to it; dragged onto the “Projects” heading, at the top level. Subprojects come along. Without dragging, use `…` → “Move to”.
- **Delete projects:** `…` → “Delete” shows first what is affected. Entries stay under “All” unless you choose otherwise. You can undo it in the activity.
- **What's new:** After an update, this window shows up once by itself. Before installing you see what is coming, and all earlier changes are under Settings → General → “Changes”.

### For AI clients
- New tools `update_project` and `delete_project`. An AI client limited to one project cannot move or delete that very project.

### Good to know
- Documentation pages and templates that belong only to a deleted project always go to the trash with it. That way a project's template does not suddenly become one for everything.

## 0.3.0 · 2026-11-01

Every project gets its handbook, and entries can point to files on your Mac.

### New
- **Project documentation:** Open a project and switch to “Docs” at the top right. There you write pages and subpages, read them with a table of contents, page back and forth and search them.
- **Mirror docs from repos:** Under “From files”, mirror a folder such as `docs` or a `README.md`. The pages stay read-only and are refreshed when you open the docs. If a drive is not there, they are kept.
- **File links:** Under “Linked files” in an entry, point to files and folders without copying them. The link still works after you move the file.

### For AI clients
- `list_docs`, `read_doc` and `write_doc`: an AI client sees the table of contents and reads only the section it needs. That saves tokens.
- `link_file` and `read_linked_file`: a link an AI client makes itself can be read only once you have allowed it in the entry. You can take access back per link.

### Fixed
- “Last changed” sorted entries from the same millisecond at random.
- The number next to a project counted documentation pages and templates. Now it counts entries only.

## 0.2.0 · 2026-10-31

The first published version.

### Knowledge graph
- Entries in Markdown, linked by `[[wikilinks]]` or named relations such as “depends on”.
- Projects at any depth, collapsible in the sidebar. An entry can be in several projects.
- Colour by project, kind or topic. The timeline plays back how your graph grew.
- Quick Capture: capture something from anywhere with ⌥⇧Space.

### Search by meaning
- Search by words in German and English, with no setup.
- Optionally a local model (about 490 MB, Settings → “Search by meaning”): it finds entries without a shared word, spots duplicates and suggests links.

### AI access
- Claude Code, Claude Desktop, Cursor and Codex can be set up with a click under Settings → “AI connections”.
- Each AI client has its own permissions and, if you like, access to certain projects only. What it writes stays marked “unreviewed” until you have seen it, and every change can be undone.
- Templates and prompts: reviewed templates from the library are also offered to AI clients as prompts.

### Security and data
- Everything is stored encrypted on your Mac, search index, attachments and backups included. Unlock with your password or Touch ID.
- Encrypted attachments with previews, daily backups with restore, and a trash.
- Automatic updates, signed and optional.
