# Changelog

All notable changes to NotebookLab will be documented in this file.

## [Unreleased]

## [0.8.10] - 2026-08-02

### Changed

- Four agreements that no compiler checks are now checked by tests: the event
  names the backend emits against the ones the frontend listens for, the theme
  colours the canvas, the notes graph and the idea space read at runtime against
  the ones the stylesheet defines, every command-palette entry against the
  sidebar, and the page headings against the sidebar item that opens them. Each
  of these fails silently when it drifts, which is what makes them worth
  pinning: a renamed event is a progress bar that never moves, and an undefined
  colour is a drawing that quietly stops following the theme.

### Fixed

- Three places named a destination differently from the sidebar item that opens
  it: the command palette offered "Transforms" where the sidebar says
  "Transform", the transforms page was headed "Content Transforms", and the
  help page was headed "Guide" while the sidebar says "Help". Landing somewhere
  headed differently from the link you followed reads as having gone to the
  wrong place. Tests now compare every palette entry and the page headings
  against the sidebar.

## [0.8.9] - 2026-08-02

### Fixed

- The first error a new user meets came in two wordings. With nothing connected
  yet, every AI feature fails, and what the user reads depends on whether the
  frontend recognises the backend phrasing. One path said "No model is connected
  yet" and offered somewhere to go; the automatic-selection path said "No
  providers registered. Set up a model first.", which matched no hint and so
  arrived bare. Both share one sentence now, and a test fails if a hint stops
  matching it.
- A damaged database stopped the app opening at all. Every failure while setting
  up the database aborted startup, so a file left half-written by a power cut or
  a killed process meant the window never appeared again, showing a line like
  "database disk image is malformed" with nothing to do about it. An unreadable
  file is now renamed out of the way, never deleted, and a fresh one takes its
  place, so the app always starts and the old file stays for anyone who wants to
  recover it. Its write-ahead log moves with it, since a log left behind would be
  replayed into the replacement. Only genuine corruption triggers this: a locked
  or unreadable file is a different problem and is still reported rather than
  hidden behind an empty library.

## [0.8.8] - 2026-08-02

### Changed

- The format names the pickers offer and the ones the backend can build are now
  checked against each other. A Studio or Audio Studio request carries its format
  as a plain string across the IPC boundary and nothing on either side is checked
  by a compiler, so adding a format to a picker without adding its prompt would
  produce a button that fails only when pressed. Both lists are read from the
  source and compared in both directions.

### Fixed

- An import that failed could never be retried. The failed document stays in the
  library so it is visible rather than vanishing, and its file hash stayed with
  it, so importing the same file again answered "this file has already been
  imported", which claims a success that never happened and left no way forward
  except finding the failed row and deleting it by hand. A failed attempt is
  replaced now, so trying again simply works. A document still processing is
  left alone, since it may belong to an import running at that moment.
- An Ollama download that died took every later one with it. The slot marking a
  pull in progress was cleared on the last line of the download thread, so a
  thread that failed hard left it occupied and the app refused new downloads,
  claiming one was already running, until it was restarted. It is released
  automatically now, however the download ends. The bundled model downloader had
  the same shape and was fixed earlier; this was its twin.

## [0.8.7] - 2026-08-02

### Fixed

- The sidebar and everything it led to disagreed on a name. Clicking Think
  opened a page headed "Thinking Partner", the Ctrl+K palette offered
  "Thinking Partner", and the Help page described it under that name too, while
  no label anywhere in the app said it. The heading, the palette entry and the
  Help section all say Think now, and the fuller phrase survives where it
  describes rather than labels.
- The Help page named a feature that does not exist. It described the "Thinking
  Partner", which the sidebar calls Think, so the page a reader opens when they
  cannot find something sent them looking for a menu item that was not there,
  and it still described the old mind map rather than the idea space that
  replaced it. Three other headings carried a definite article the sidebar does
  not. All of them now match, and a test reads the sidebar and fails if any
  user-facing copy drifts from it again; the same drift had already happened
  once in the first-run sample notes.
- The Vite config used __dirname, which the native config loader Vite is moving
  to does not define, and which it warned about on every build.

### Fixed

- Opening the Documents page outside the packaged app took the whole window
  down with "Something went wrong". Drag-and-drop asks Tauri for the current
  webview, which throws rather than returning nothing when the globals it reads
  are absent, and the error escaped the effect into the page. Everything that
  reaches for the event bridge or the webview now checks first, through one
  helper rather than four copies of the same condition, so a missing bridge
  costs that one feature instead of the page. The packaged app always injects
  those globals, so this was only reachable in a browser, which is where the
  layout gets checked during development.

### Removed

- Two commands that nothing called. get_job returned a single job by id, which
  the job store never asked for because it reads the whole list and then follows
  the event stream, and download_default_model fetched a hardcoded model that
  the catalog now carries and the Models page downloads by id. Both were exposed
  over the IPC boundary while being unreachable from any interface. With the
  second gone, the download helper no longer needs optional arguments or the
  fallback branches behind them.

## [0.8.5] - 2026-08-02

### Changed

- Dependency audit across both ecosystems. npm reports no vulnerabilities and
  every package is on its latest release except TypeScript, which is held at 6
  because typescript-eslint 8.65.0, the newest there is, still declares support
  only up to TypeScript 6.1: moving to 7 would ship with linting broken.
- Rust crates updated where the change was verifiable: rusqlite 0.32 to 0.40,
  zip 4 to 8, quick-xml 0.39 to 0.41, pdf-extract 0.10 to 0.12. The docx tests
  write a real zip and read it back through quick-xml, so those two are checked
  by more than compilation.
- quick-xml 0.41 requires the XML version at each call and deprecated
  unescape_value. Word writes XML 1.0 and the parser never reads the
  declaration, so 1.0 is stated explicitly and the deprecated call is gone.
- reqwest is held at 0.12 deliberately. Version 0.13 replaces the default TLS
  backend with rustls and aws-lc-rs, which pulls a C toolchain and cmake into
  the build and moves certificate verification off the operating system trust
  store. A machine behind a corporate proxy with its own certificate authority
  would stop reaching cloud providers, and no test here could catch it.
- sha2 is held at 0.10 deliberately. Version 0.11 changes the digest output
  type, and that value is the file hash used to recognise a document already
  imported; a changed hash would make every existing document look new.
- rten stays at 0.24 because ocrs, which reads scanned pages, requires it.
- Every GitHub Actions pin updated: checkout and setup-node to v7, the Pages
  actions to their current majors, rust-cache and rust-toolchain to their
  current commits. Dependabot had been told to ignore major bumps for all
  actions, which did not keep them stable, it kept the drift invisible; only
  tauri-action, which holds the signing key, stays held back now.
- The REST API server no longer contains an unwrap. Both were building a header
  from compile-time constants and could not fail, but a server thread with no
  panic path at all is a simpler thing to reason about.

### Changed

- Home no longer repeats what is already on screen. It opened with the mark and
  the word NotebookLab as an eyebrow, an inch below the header that carries both
  permanently, and a line about everything staying on your machine that the
  empty state repeated directly beneath it. The greeting now stands on its own.
- The four quick actions say something their labels do not. "New note" was
  captioned "Start writing", which is the label again, and importing named
  three formats when the app also reads Word documents and images.
- One word for one thing. The status bar counted "chunks" while Home counted
  "passages" and the document view said both, for the same number from the same
  place. It is a passage everywhere the user can see it, and chunk only in the
  code and the database, where it is the accurate term.
- Home no longer runs its own count of indexed passages. It duplicated the one
  in the status bar, which is on screen at all times, and cost an extra call on
  every visit to say the same number twice.

### Fixed

- The tour ran off the bottom of the window. It positioned each card using a
  fixed guess of 210 pixels for its height, and the cards actually measure
  between 236 and 290, so the clamp that was meant to keep them on screen
  believed they fitted when their lower half was already past the edge, taking
  the Next button out of reach. The card is now measured, flips to the other
  side of what it is pointing at when there is no room, and scrolls rather than
  overflowing on a window too short to hold it.
- The notebook being worked in is shown in the header at all times, beside the
  app name. It was in the status bar in the smallest type on screen, among
  transient counters, and only appeared once a notebook had been chosen, so the
  state that most needed saying, none selected, was the one that said nothing.
  It now names the notebook with its own colour, or offers to choose one, and
  the duplicate has been taken out of the status bar.

- Saving an audio file on Windows put a PowerShell console window on top of the
  app for the whole of synthesis, which for a full script is many seconds of a
  black box the user did not ask for. The speech process is now started hidden,
  the same way the graphics probe already was.

## [0.8.4] - 2026-08-01

### Fixed

- Generation over a whole notebook read only one document. When no sources were
  picked by hand, the Studio and the Thinking Partner asked for a spread of
  passages across the notebook, but the query ordered every chunk by document
  date and took the first twenty, which is the opening of the newest document
  and nothing else: any real document has more than twenty chunks, so the rest
  of the library was never reached. A study guide, a mind map and a briefing
  built from a ten-source notebook were all built from one source, and all from
  the same opening passages, which is a large part of why different formats came
  back so alike. The spread is now taken evenly across every document, the same
  way an explicit selection already was.

- A model download that stopped early was installed anyway. A body that ends
  before its declared length arrives as a clean end of stream rather than an
  error, so the partial file was renamed into place, and because the check for
  an existing model only asks whether a file is there and non-empty, nothing
  ever replaced it. The result was a corrupt model that failed to load with no
  sign of why. The size is now confirmed before the file is put in place, and a
  short download is deleted and reported.
- A download that failed hard could block every later one. The flag marking a
  download in progress was cleared by hand on each path out, so a panic in the
  download thread left it set and the app then refused new downloads, claiming
  one was already running, until it was restarted. It is now released
  automatically however the download ends.
- A download from a server that sends no content length showed "12 MB of 0 B
  (0%)" with the bar stuck at nothing while it was in fact running. It now
  reports the bytes it has.

### Fixed

- Automatic model selection sized a request by bytes divided by four when
  deciding whether a provider's context window could hold it. That rule reads a
  CJK character as three quarters of a token when it is nearer one, so a request
  in those scripts looked about a quarter smaller than it was and could be routed
  to a window it would overflow, which a server answers by truncating rather than
  refusing. The router and the RAG packer now share one estimator, so they cannot
  disagree about how big a prompt is.
- The GPU probe behind the Models page could wait forever. It shells out to
  nvidia-smi, which hangs when a driver is wedged or a card is mid-reset, and
  nothing bounded that wait, so an optional detail could stall the page a new
  user needs to reach a working model. It now gives up after five seconds and
  falls back to recommending by RAM.
- A graphics card whose name contains a comma is read correctly, and a card that
  reports no memory figure is still named rather than discarded.

## [0.8.3] - 2026-08-01

### Fixed

- The sample notebook sent new users to features by the wrong name. The welcome
  note told them to open the "Thinking Partner", which the sidebar labels
  "Think", so the first thing a new user was told to do could not be found. The
  sample content now names every tool exactly as the sidebar does, and a test
  reads the sidebar to keep the two from drifting apart again.
- Notebook bundles exported by a version before 0.8.1 carried scrambled chunk
  order, and importing one after the repair migration had already run left it
  scrambled for good. Imports now rebuild the reading order rather than trusting
  the bundle.
- Chat history had no tie-break in its ordering. Two messages written inside the
  same clock tick carry the same timestamp, and which came back first was then
  left to the query planner. The window of recent messages the model is given as
  its memory of the conversation had the same gap in both directions, where a tie
  resolved inconsistently would drop one message and repeat another. Both now
  order by insertion as well as time.
- The release guard checked three manifests for the version but not the lock
  file, which had sat at 0.7.7 through three releases. Nothing built with
  --locked, which is the only reason it did no harm. The lock file is now part
  of the check, and the release notes no longer ask for a README badge that
  updates itself.
- The local REST API opened its database with no busy timeout, so a checkpoint
  overlapping a request failed it outright with a locked database instead of
  waiting the moment out.

## [0.8.2] - 2026-08-01

### Fixed

- Documents imported before 0.8.1 are repaired in place on first launch. The
  previous release stopped new imports from being stored out of order, but left
  every document already in the library scrambled, fixable only by knowing to
  re-import it. A migration now renumbers those chunks into reading order.
  Documents that were already numbered correctly are left untouched.

Re-importing is still worth it for documents in Chinese, Japanese, Korean, Thai
or Lao added before 0.7.5: those were stored as a single chunk, and only a fresh
import can split them.

## [0.8.1] - 2026-08-01

### Fixed

- Multi-page documents were read out of order. Chunk positions were numbered from
  zero for each page, so a ten-page PDF held ten chunks numbered 0, ten numbered
  1, and so on. Everything that reads a document back orders by position, so the
  pages interleaved: a transform assembled the first chunk of every page, then the
  second chunk of every page, rather than the document in reading order. Positions
  now run continuously across the whole document.

### Changed

- Removed autoprefixer and postcss, which Tailwind 4 makes unnecessary. The
  generated stylesheet is byte-identical without them, which is what confirmed
  they were doing nothing.


## [0.8.0] - 2026-08-01

### Added

- The Studio and the Audio Studio keep every result. Generating a quiz used to
  throw away the study guide made a minute earlier, so reading it again meant
  waiting for the model a second time. Each format now holds its own output, and
  a small mark on the format buttons shows which ones a notebook already has.

### Fixed

- Models wrapped their answers in XML tags of their own invention, so a set of
  key points arrived with `<extraction_results>` printed above and below it. Every
  prompt here delivers source text inside tags, which keeps it as data rather
  than instructions, and models mirror that style back. The wrapper is stripped
  from every generation, and the prompts now say not to add one.

### Changed

- Upgraded to Tailwind CSS 4, TypeScript 6, and ESLint 10, with every other
  dependency on its latest release and 57 Rust crates refreshed. `baseUrl` was
  removed from the TypeScript config, having been removed from TypeScript itself.
- TypeScript 7 is deliberately not used: the lint toolchain supports up to 6.1,
  and moving past it would mean shipping with linting broken.


## [0.7.7] - 2026-07-30

### Changed

- The local model server's lifecycle is now covered by tests: that a second start
  cannot win the race and spawn a second process, that a crash is recoverable
  without restarting the app, and that a stop the user asked for is never
  reported back to them as a crash.
- Replacing the handle to a running server kills it first. Dropping that handle
  does not stop the process it refers to, so if the start guard were ever
  loosened the result would be a llama-server running with nothing holding it,
  invisible until the memory was noticed.


## [0.7.6] - 2026-07-30

### Fixed

- The prompt budget under-counted text in CJK by about a quarter, because it
  estimated tokens from byte length and a character there is three bytes but
  roughly one token. Under-counting is the dangerous direction: it builds a
  prompt larger than the window, and a server does not refuse that, it truncates
  the system prompt away.

### Changed

- Search now has tests covering the path CJK actually takes. The full-text index
  treats a run of those characters as one token, so a partial match finds nothing
  through it and the LIKE fallback is what rescues the search. That fallback
  looks redundant next to a full-text index and is not; it is now pinned, along
  with search staying inside its own notebook and a wildcard typed into the query
  not matching everything.


## [0.7.5] - 2026-07-30

### Fixed

- Documents in Japanese, Chinese, Korean, Thai and Lao were not being split into
  chunks at all. Length was measured by counting whitespace-separated words, and
  those scripts are written without spaces, so five thousand characters of
  Japanese counted as a single token: the target was never reached and an entire
  document became one chunk. Retrieval then had nothing to rank, and every
  citation pointed at the whole file rather than a passage. Characters in those
  scripts are now counted individually.
- Sentence splitting only recognised the ASCII full stop, so it never fired on
  text ending in the full-width stop those languages use. The last-resort split
  had the same blind spot: it broke on spaces, which that text does not have.
- Chunk overlap took the last N whitespace-separated words, which for an unspaced
  script is the entire chunk, so every chunk after the first began with a
  complete copy of its predecessor.

Text in spaced scripts counts exactly as it did before, so nothing already
indexed changes shape.


## [0.7.4] - 2026-07-30

### Fixed

- Chat answers were grounded in fewer sources than they should have been. Packing
  the context reserved 2048 tokens for the answer while a local model was only
  ever asked for 900, so on the conservative 4096-token window more than a
  thousand tokens were held back for an answer that could not use them and
  passages that would have fitted were dropped. One function now decides the
  allowance and both the packing and the request read it.
- A long conversation could push the prompt past the context window. The floor
  that was meant to guarantee some room for sources added 512 tokens on top of a
  prompt that was already too big; a server does not reject that, it truncates
  from the front, which is where the system prompt lives, so the model lost the
  instruction to cite its sources exactly when the prompt was most crowded. No
  sources are sent now when there is no room for them.
- A single very long passage could overrun the window on its own, because the
  highest-ranked chunk was admitted regardless of its size.


## [0.7.3] - 2026-07-30

### Changed

- The local REST API decides authentication as "everything except the health
  check" rather than by listing the routes that need it, so a route added later
  is private by default. The previous shape is how an endpoint ends up public by
  omission. The token comparison and the public-path decision now have tests.
- The full-text search sanitiser has tests. It is what stops a search query being
  parsed as FTS5 syntax, and a regression there would be a query injection with
  nothing to catch it.


## [0.7.2] - 2026-07-30

### Fixed

- Importing a shared notebook could write a canvas of any size straight into the
  database. The ceiling that stops embedded images growing the database without
  bound lived in the save command, so the import path walked past it. The limit
  now sits with the write itself, where no caller can route around it.
- A notebook file with no name imported as a notebook with nothing to click, and
  an absurdly long name broke every list it appeared in. Names are trimmed,
  required, and capped.
- The download host check compared the host case sensitively, so a legitimate
  link written `HuggingFace.co` was refused, and it kept any userinfo and port in
  the string it compared. It now parses the authority properly: a subdomain is
  allowed because Hugging Face serves files from a CDN, a lookalike domain is not,
  and text before an `@` cannot disguise the real host.

### Changed

- Notebook export and import, and the download host check, now have tests. Both
  had none, and both are places where a defect either loses a user's data or
  decides what gets written to disk and run as a model.


## [0.7.1] - 2026-07-30

### Fixed

- The Idea Space reached for a theme colour that does not exist, so evidence fell
  back to a hardcoded green that ignored light mode. Evidence is drawn as a square
  now: shape rather than a fourth colour, which also survives colour blindness.
- The canvas read theme variables for every node and every edge on every frame,
  hundreds of style reads a second for colours that cannot change mid-frame.
- The layout simulation ran forever, so the picture kept drifting under the
  reader. It cools and settles now.
- Clicking an idea recomputed the projection separately from the drawing, the same
  maths written twice and free to drift apart. Clicks reuse what was drawn.
- A model's reply is cleaned before it is drawn. A duplicate id silently lost an
  idea, an edge naming an id that was never defined drew to nowhere, and a
  self-edge made the layout push a node against itself.

### Changed

- The changelog records 0.5.0 through 0.7.0, which shipped undocumented.
- The README, the landing page and the page's own header described the Thinking
  Partner as making mind maps, which it no longer does.


## [0.7.0] - 2026-07-30

### Added

- **Idea Space** in the Thinking Partner: the claims, evidence, tensions and
  open questions across your sources, laid out in three dimensions and turnable.
  Contradictions are drawn dashed and held further apart, because the distance
  is part of what the picture says; open questions are drawn hollow, because they
  are the holes in the argument. Click an idea to park its label where it stays
  still.
- A voice for each speaker in the Audio Studio, chosen by you. The automatic
  pick ranks by quality markers, which cannot settle taste, and on a machine with
  several good voices the pair matters more to how human it sounds than any
  pacing does.
- The Audio Studio now says when only basic system voices are installed, and
  where to get the free natural ones. No amount of pacing rescues a formant
  synthesizer from 1995.

### Changed

- The Thinking Partner had been drawing the Studio's mind map from the Studio's
  prompt, so both features produced the same picture from the same sources and
  one of them had no reason to exist. The Studio keeps the mind map, which is
  what an outline of a topic should be. Idea Space asks a different question: not
  what the sources contain, but how they disagree.
- Audio scripts are written for the ear rather than the page: short sentences,
  contractions, numbers said the way a person says them, and commas where a
  speaker draws breath. A synthesizer pauses at punctuation and nowhere else, so
  punctuation is the only pacing available to it.
- Delivery varies slightly from sentence to sentence, derived from the sentence
  itself so a script always sounds the same on replay. Identical settings for
  every sentence is most of what makes read-aloud drone.

## [0.6.1] - 2026-07-29

### Fixed

- Every feature froze at 17% and produced nothing. Streaming had moved progress
  reporting inside the token callback, which made the bar depend on the model
  saying something: a quiet stream left it stopped at the phase boundary with no
  elapsed time, which is indistinguishable from the app having died. The timer
  always runs now and takes whichever signal is further along, and a stream that
  fails or produces nothing is retried without streaming.

## [0.6.0] - 2026-07-29

### Added

- Answers stream. Chat shows the reply being written rather than a bar, and
  progress is now measured from tokens actually received instead of extrapolated
  from how long previous runs took. Providers that cannot stream fall back to the
  estimate and behave as before.

## [0.5.4] - 2026-07-29

### Fixed

- A local model server started after opening NotebookLab was never noticed.
  Detection ran once during startup and then stopped, so the app insisted there
  was no model while one sat there answering, and the only way out was knowing
  that Models has a rescan button. It watches continuously now and picks up a
  server within a few seconds of it starting.
- Voices for read-aloud were chosen by matching names against a list of guesses,
  which ignored the neural voices Windows and macOS ship alongside the old
  robotic ones. They are ranked by quality now.
- Long turns are split into sentences before being spoken. Several speech engines
  silently truncate an utterance past a few hundred characters, so a long answer
  simply stopped partway through.
- A real beat between speakers. Turns ran together with no gap, which made an
  exchange sound like one person reading a transcript aloud.

## [0.5.3] - 2026-07-29

### Fixed

- Chat showed your question twice once the answer arrived.
- Chat's progress appeared in whichever conversation you opened next, not the one
  that had asked.
- A transform result was shown against the document selected when it arrived
  rather than the one that produced it, so a summary of one document could be
  presented as another's.

## [0.5.2] - 2026-07-28

### Changed

- Chat runs as a tracked job like every other feature. The answer was always
  saved either way, so the work was never lost, but nothing told you it was still
  coming: leaving Chat and returning showed your question sitting alone with no
  reply and no sign one was on its way.

## [0.5.1] - 2026-07-28

### Fixed

- Every feature failed at exactly four minutes with "the model did not answer in
  time" while the model was still writing. The request ceiling was four minutes,
  chosen when there was no progress bar to look at; it is thirty now, and the
  work is cancellable.
- Requests are sized to the machine that will answer them. A local model on a CPU
  was being handed the same eight thousand tokens of sources and asked for the
  same two thousand words as a hosted model, which is fifteen minutes of real
  work before the first word appears. Answers from a local model are shorter as a
  result, which beats a longer one that never arrives.

## [0.5.0] - 2026-07-28

### Added

- Generations run as tracked background jobs. Each reports what it is doing, a
  percentage weighted so it advances at an honest rate, and an estimate once
  there is enough signal to give one. Leaving the page no longer throws the work
  away, and the status bar shows what is running from anywhere in the app.
- Choose which documents a generation reads. It used to sample whatever the
  notebook happened to hold, capped at twenty chunks, with no way to say "use
  this one" and no way to tell afterwards what it had read.
- Every AI page states which notebook it is working in, how many documents are
  readable, and how many are still processing.
- Save what a feature produced: the Studio formats, the Idea Space, Socratic
  questions, transforms, a chat conversation, the canvas as a PNG, and an audio
  script as a transcript or as a real audio file written by the platform's own
  speech engine.

### Fixed

- Drag and drop was dead. Tauri's drag-and-drop was switched off, which killed
  the events the document import listened for, while the canvas used the opposite
  mechanism; a feature described in the CHANGELOG and on the landing page could
  not fire. One mechanism now serves both.
- The Audio Studio failed with "the model generated an empty script" on output
  that was perfectly usable. The parser accepted only bare `A:` and `B:` prefixes,
  so a model emitting `**A:**`, a list marker, or a line of preamble produced
  nothing at all. It now accepts what models actually write, and reads unlabelled
  prose as a single narrator rather than discarding it.
- A document with no blank line in it became one unbounded chunk, which meant a
  single enormous row to index and a single enormous prompt to send.
- The transform prompt budget was tested after appending rather than before, so
  it was always exceeded by one chunk.
- Only the first detected local provider was registered, so a machine running
  both Ollama and LM Studio showed one of them.


## [0.4.6] - 2026-07-27

### Added

- Three ways to hear a notebook, alongside the existing four. An **interview**
  puts an interviewer and an expert on the detail, a **lecture** teaches the
  material in order from the first idea to what is worth remembering, and
  **questions** runs through what the material genuinely raises with a direct
  answer to each.

### Changed

- Audio overview is now the **Audio Studio**, sitting alongside the Prompt
  Studio, and it says what it does: your notebook, read aloud, told the way you
  choose.

### Fixed

- Thinking Partner and the Studio no longer claim a full notebook is empty.
  Both searched by keyword and refused to continue when nothing matched, so a
  topic worded differently from your documents, or simply mistyped, produced
  "no relevant documents found" beside a notebook with sources in it. When a
  topic matches nothing they now work from a spread of the notebook instead,
  and only a genuinely empty notebook is treated as an error.
- A local model that cannot answer no longer holds a request for ten minutes
  before automatic selection moves on. The ceiling for models running on this
  computer is now four minutes, which is generous for a model sized for the
  machine and short enough that fallback feels like a pause rather than a
  hang.

## [0.4.5] - 2026-07-27

### Security

- React Router moves to version 8, which closes a high severity advisory in
  the 7.x line (an RSC mode CSRF bypass that could run an action before a
  rejected request returned). The routing package was consolidated upstream,
  so `react-router-dom` is replaced by `react-router` throughout. The app uses
  only the standard navigation pieces, and behaviour is unchanged.

### Changed

- Dependencies updated: React 19.2.8, TanStack Query 5.101, jsdom 30,
  Testing Library jest-dom 7, PostCSS 8.5.23, typescript-eslint 8.65, and
  serde_json 1.0.151.
- Automated dependency updates are quieter and safer. They now arrive monthly,
  grouped into a single pull request per ecosystem rather than up to thirteen
  at once, and major bumps are excluded so a breaking change cannot block a
  batch of otherwise safe updates.

### Added

- Test coverage for the accessibility preferences, the autosave registry, and
  the keyboard shortcut registry.

## [0.4.4] - 2026-07-27

### Fixed

- Cloud models answer again. Connecting a provider only activated it when a
  four-second availability probe happened to succeed, so a valid Gemini key
  could sit saved but inactive while a slower local model kept answering.
  Connecting now always activates, and the probe only shapes the message you
  see.
- Gemini no longer returns empty replies. Flash models spent their whole
  output budget on internal thinking and came back with nothing; when a reply
  is genuinely empty the app now names the cause instead of showing a blank.
- Retired Gemini model identifiers have been refreshed, so a model the
  provider has since withdrawn no longer fails with an unexplained error.
- Newer Gemini key formats are accepted. Validation checked a single prefix
  and turned away keys issued under the newer one.
- Provider failures explain themselves. A refused request carries the
  provider's own reason through to the interface, with a hint pointing at the
  usual causes: the key, the quota, or the model.

### Changed

- Local models start on their own. A model starts once its download finishes,
  a one-click banner starts it when it is stopped, and the last local model is
  restored on launch.
- Model suggestions favour responsive sizes. A 16 GB machine was being pointed
  at a 7B reasoning model that takes minutes per answer on CPU; suggestions are
  now capped at sizes that reply promptly.
- Local reasoning output no longer shows the model's internal monologue, and
  the sidecar's context window has doubled to 4096 tokens.
- Timeouts say plainly that they are timeouts, and local models are given
  longer before one is declared.
- Dependencies updated: five npm packages, five cargo crates, sysinfo 0.33 to
  0.39, and the checkout action pinned.

## [0.4.3] - 2026-07-20

### Fixed

- The note editor's toolbar and auto-save now actually work. Change listeners
  were registered on a manager the editor replaced during startup, so edits
  were never reported and notes could silently fail to save. Every toolbar
  command is now verified live against the running editor.
- Closing the window closes the app. The close handler needed a window
  permission the app never granted, so the close click was swallowed and the
  local AI server kept running. One click now closes the window and shuts the
  server down with it.

### Added

- A fuller formatting toolbar in notes: Heading 3, task lists with clickable
  checkboxes, tables, code blocks, dividers, and links with inline URL entry.
- Toolbar buttons light up to show the formatting active at the cursor.
- Markdown copy and paste keeps its structure, and typing shortcuts like
  `# `, `- [ ] `, and `> ` convert as you write.

## [0.4.2] - 2026-07-18

### Added

- Attach a file straight from the chat box, like a chat app. The button opens
  the file picker and adds the document to the notebook through the same import
  that drag and drop uses, so the next question can draw on it.
- Check for updates from Settings. It asks GitHub for the latest release,
  downloads it if there is one, and offers a restart to apply it.

### Changed

- Softer, rounded corners on buttons and inputs across the app, so it reads
  less boxy.

## [0.4.1] - 2026-07-18

### Fixed

- The app no longer freezes on startup. Detecting local model servers used to
  hold a lock while probing the network; a slow or filtered port could stall
  every feature with no output and no error. Probing now runs without the lock,
  so chat and everything else stay responsive.
- A model you connect now activates immediately, so a fresh Gemini key or a
  downloaded model works without an extra step.

### Changed

- Every feature remembers where you left off. Leave Chat for the Studio or
  Audio and come back, and your conversation, transform, overview, mind map,
  and last search are still there, so a notebook feels like one workspace.
- Notes edit like a notepad: a formatting toolbar, a visible blinking cursor,
  click anywhere to write, and a wider, roomier layout.
- Accessibility settings: a text and interface size slider and a high-contrast
  mode, both remembered across restarts.

## [0.4.0] - 2026-07-17

### Added

- Proven methods, built in. Prompt Studio now loads the open AI-SKILLS
  library (github.com/Amey-Thakur/AI-SKILLS) automatically: pick a working
  method — code review, debugging, research synthesis, and more — and it is
  inserted into your description in plain sight for the crafter to build on.
  The catalog is cached for a day, only that one repository is ever
  contacted, and everything degrades gracefully offline.

- Share a notebook. Export any notebook to a single self-contained file that
  holds the notebook, its notes, its documents (as their extracted, searchable
  text), and its canvas, then import that file on another machine to recreate
  the notebook, fully offline and with no original source files needed. Import
  and export live on the Notebooks page; a half-finished import is rolled back
  so nothing partial is left behind.
- Audio overview formats. The audio overview (read aloud in the browser) can now
  be a two-host discussion, a one-minute brief from a single narrator, a debate
  between opposing speakers, or a critique that weighs the material's strengths
  and gaps. Each is grounded in the notebook's sources, and the prompts now keep
  document text strictly as data.
- More Studio formats. Alongside the study guide, flashcards, quiz, and mind
  map, the Studio can now turn a notebook's sources into a timeline, a slide
  deck, a data table, a briefing doc, and a blog post. Each one is grounded in
  your own documents and renders in its own real view, with the timeline, deck,
  and table laid out visually and the reports read as formatted prose.
- Canvas workspace. Every notebook now has one open spatial canvas: draw
  freehand with a pressure-aware pen, add rectangles, ellipses, and text, drop
  in images, and arrange it all together. Pan by dragging the empty space, zoom
  toward the cursor, select and move or delete anything, and undo/redo. The
  whole scene, images included, is stored with the notebook and autosaves as you
  work, so it stays self-contained and offline. Built on a small custom SVG
  engine and perfect-freehand rather than a heavy whiteboard library, so it
  matches the app's look and adds almost nothing to the bundle.
- Word and image import. Bring in Word (`.docx`) files, and images or scans of
  printed text (`.png`, `.jpg`, `.jpeg`, `.tiff`, `.webp`, `.bmp`). Images are
  read with fully offline OCR, no cloud and no network, so a photo or scan
  becomes searchable content like every other source and flows into search,
  chat, and the Studio. The OCR models are bundled and verified by checksum; if
  they are ever missing, image import degrades to a clear message instead of
  failing quietly. Word and text formats never load the models.
- Studio: turn a notebook's documents into study aids grounded in your own
  sources. A structured study guide, interactive flashcards, a scored
  multiple-choice quiz, and a real visual mind map. Add a focus to narrow it or
  leave it blank to cover the whole notebook.
- Visual mind maps: the mind map now renders as an actual tree of connected
  ideas, in the Studio and in the Thinking Partner, replacing the text preview.
- Home: a calm landing screen with a greeting, quick actions, your recent
  notes, and a preview of your notebooks.
- Universal search launcher: one keyboard-first box (Ctrl+K, or the header
  Search button) to reach any page, notebook, or action.
- Keyboard shortcut system with a shared registry and a cheat sheet: press `?`
  anywhere to see every binding, grouped by area. Navigate by typing `G` then
  a key (`G` `N` for Notebooks, `G` `A` for About, and so on). The Settings
  page reads the same registry, so the list can never drift from what is wired.
- First-run welcome: a short, spacious greeting on the first launch that
  introduces the app, lets you pick a theme, and points out the keys worth
  knowing. Shown once.
- Animated light and dark toggle in the header: a real switch whose knob slides
  between a sun and a moon, with the active side lit in the accent color.
- About page: the people behind NotebookLab, why it exists, portraits pulled
  live from GitHub, and the Makers' Pledge, a certificate carrying the
  fingerprint of the key that signs every commit and release.
- The Makers' Pledge as a signed certificate of authenticity: shown on the
  website and in the README, and available as a one-click download.
- A single brand mark component, drawn from the packaged app icon, now used
  consistently in the header, the welcome flow, and the About page.
- A shared, accessible dialog primitive (focus trap, Escape to close, reduced
  motion aware) behind the welcome and cheat sheet overlays.

### Changed

- The Models page breathes. A wider layout, a three-column first-visit
  guide, roomier cards, and no more text wrapping awkwardly mid-card.

- Eight bundled models instead of one. The built-in offline server now offers
  a curated, size-verified catalog (Llama 3.2 1B and 3B, Gemma 3 4B, Phi-4
  Mini, Qwen 3 4B, Mistral 7B, Qwen 2.5 Coder 7B, DeepSeek R1 Distill 7B),
  each with its memory needs and strengths, downloadable in one click with
  live progress. NotebookLab reads this computer's memory and marks the
  strongest model it runs comfortably as "Recommended for this computer",
  and the Local AI Server card grew a picker to start any downloaded model.
  All of it runs fully offline with no account and no token bill.
- Developer logs in Settings. An Advanced section shows the backend's recent
  activity (model detection, imports, downloads, errors), kept in memory
  only, with refresh and copy-all. Useful for bug reports and for anyone
  building against the local REST API.

- Auto model selection now also respects context limits: a request that could
  not fit a model's known context window is routed to a model with room
  before it is ever sent, so long conversations stop degrading quietly.

- Auto model switching. Turn on Auto in the model menu and NotebookLab picks
  the best of your connected models for each task: quick jobs go to fast,
  free local models, hard generation goes to your strongest model, and if one
  stops answering the request quietly falls over to the next best. The menu
  always shows which model actually served the last request, and choosing a
  model by hand simply switches Auto off. The choice is remembered.
- Live token usage. A counter in the status bar shows this session's real
  token use, exactly as reported by the providers, never estimated. Click it
  for the last request's context-window fill (shown as a percentage only when
  the model's window is a known fact) and a per-model breakdown with each
  model's share. It appears only once the AI has done something, and resets
  when the app closes.
- A 3D connections map. The Connections page can now draw the notebook's note
  links as a slowly turning three-dimensional cloud, built with no external
  libraries: drag to rotate, scroll to zoom, click a note to open it, with
  the flat view one click away and the choice remembered. It stands still
  under reduced motion.
- The AI now knows your map. Chat context includes a compact summary of how
  the notebook's notes link together, so answers can lean on the shape of
  your work, not just isolated passages.
- Smarter context packing. Retrieved passages are fitted to the active
  model's actual context window (falling back to a safe floor when the
  window is unknown), most relevant first, and citations only ever refer to
  passages the model really received.

- Open files with NotebookLab. The installer now registers the app for PDF,
  Word, text, and Markdown files, so "Open with NotebookLab" from your file
  manager drops the file into your current notebook, indexed and ready to ask
  about. Opening a file while the app is running reuses the same window, and
  several files opened at once import one after another.
- A quiet boot screen. The window shows a single pulsing mark in your theme's
  colors while the app gets ready, instead of a blank flash, and it stands
  still when your system asks for reduced motion.
- Faster startup. Every page now loads on first visit instead of all at once,
  so the app opens lighter and heavy features (the editor, the canvas, the
  connections graph) fetch their code only when you actually go there, behind
  the same quiet loading mark.

- A model menu in the top bar. It always shows which AI model is doing the
  work, and switches between your local and cloud models in two clicks, with
  search, availability dots, and pin-to-top favorites. Your choice is now
  remembered across restarts.
- First-class cloud providers. Connect Anthropic (Claude), OpenAI (GPT),
  Google Gemini, or DeepSeek through a guided three-step setup that explains
  what an API key is, links straight to each provider's key page, is honest
  about cost and free tiers, and tests the connection before calling it done.
  Anthropic and Gemini speak their own native APIs. Keys are stored only on
  this machine, are never shown again after saving, and can be changed or
  removed anytime.
- A curated local model catalog. With Ollama installed, the Models page offers
  hand-picked models across the Llama, Gemma, Qwen, DeepSeek, Mistral, and Phi
  families, each with its size, memory needs, use cases, and a quality rating,
  installable with one click and live download progress. NotebookLab detects
  this computer's memory and marks each model as fitting, tight, or too large,
  and asks before installing one the machine cannot comfortably run.
- Ollama management built in. The Models page now shows whether Ollama is
  installed and running (with friendly guidance when it is not), lists
  installed models with their disk sizes and total storage use, and can
  activate or delete any of them.
- Saved providers. Registered providers and cloud connections now survive
  restarts instead of living only in memory, so an API key is entered once.

- Guided "how it works" tour. On the first launch, right after the welcome, a
  coach-mark tour walks through the whole app in twenty short steps: every
  sidebar destination (Home, Notebooks, Documents, Search, Connections, Chat,
  Think, the Studio, Canvas, Transform, Audio overview, Prompt Studio, Models,
  Settings, and Help/About), plus universal search, the activity indicator, and
  a note that your work autosaves. Models is called out as the first stop, since
  the AI features need a model. Each step spotlights the real element, with Back,
  Skip, and arrow-key navigation. It ends by opening the Getting Started notebook
  so you can try things right away, and it replays any time from Settings.
- Your work saves itself, everywhere. Notes and the canvas already autosaved as
  you type and on leaving a page; now the app also flushes any pending edit
  before it closes, so nothing is lost even on a hard quit. The tool pages
  (Prompt Studio, Transforms, the Studio, and Audio overview) keep the text you
  type in a draft, so navigating away or reloading no longer discards it.
- Set-up guidance. The AI features (Chat, the Studio, Thinking Partner,
  Transforms, Prompt Studio, and Audio overview) now show a calm prompt with a
  one-click link to set up a model when none is loaded, instead of only erroring
  when you try to use them.
- Chat scope. Chat now shows which notebook and how many sources it is answering
  from, and points you to add one when the notebook has none yet.
- Sample content on first launch. The Getting Started notebook now ships with
  two short sample documents, already indexed, alongside its notes, so you can
  try search, chat, the Studio, and transforms before importing anything of
  your own.
- Jump back in. The home screen surfaces your recent notes and documents
  together, newest first, so you can reopen what you were working on in one
  click.
- Personal greeting. NotebookLab asks your name during first-run setup and
  greets you by it on the home screen. Change it any time from Settings; it
  never leaves this machine.
- Collapsible, grouped sidebar. Navigation is organised into Home, Library,
  Tools, and System, every item carries an icon, and the whole sidebar collapses
  to an icon-only rail from a toggle at the bottom, remembering your choice.
- Live activity indicator. The status dot now shows what the app is doing: amber
  with no model, green when ready, and a pulsing accent with a "Thinking" label
  while a chat, import, or generation is in flight, plus model-download progress.
  It respects reduced-motion settings.
- Drop files onto Chat. Dragging a file onto the chat imports it into the active
  notebook as a source (images read with OCR) and indexes it, so the next
  question can draw on it.

- Prompt Studio rebuilt as a real prompt crafter. Describe the job in plain
  words and it writes the complete, ready-to-run prompt, choosing the right
  technique for the task: worked examples for classification and extraction,
  step-by-step reasoning for logic, role and style for creative work, and
  structured output contracts where they reduce errors. Every result comes with
  recommended model settings (temperature, top-p, top-k), the variables to fill
  before use, and short notes on why it is built that way. Unknown specifics
  become named variables instead of invented facts, so prompts are accurate and
  reusable. The build-from-parts composer remains, now upgraded by the same
  crafter.

### Fixed

- A model download in progress is no longer forgotten when you leave the
  Models page: coming back restores the live progress bar, and a download
  whose connection stalls outright now ends with a clear error instead of
  silently blocking future downloads until restart.
- Chat now sends your actual question to the model. The retrieval pipeline
  stripped the current question along with the duplicate guard and never re-added
  it, so every answer was generated without the model ever seeing what was asked.
- Provider URL validation now parses URLs properly, closing an SSRF gap where a
  crafted userinfo or IPv6 host could slip past the private-network guard, and no
  longer wrongly rejects a legitimate IPv6 loopback provider.
- Listing providers and switching models no longer block the main thread on
  network calls, so an offline or slow provider cannot freeze the window.
- Plain-text import detects a byte-order mark and decodes UTF-16 files (common
  from Windows Notepad) instead of turning them into garbage.
- Every Studio view (mind map, timeline, slide deck, data table, quiz, and
  flashcards) tolerates malformed model output instead of crashing the whole app,
  coercing any stray non-text field to a string before it is rendered.
- Download buttons work: the certificate downloads as a bundled file and external
  links open in your browser.
- Dragging a Word document or an image onto the window imports it, matching the
  formats the picker already accepts.
- A single failed embedding no longer aborts a document's whole indexing pass; it
  skips the chunk and continues, giving up only after repeated misses.
- The local REST API accepts an Authorization header in any letter case, as HTTP
  requires.
- Backlinks resolve as soon as a linked note is created, including the
  click-a-link-to-create-it flow, rather than only after the source is re-edited.
- Search surfaces backend errors instead of silently showing nothing, and no
  longer queries on a whitespace-only term.
- Deleting a notebook clears its notes and documents from the "pick up where you
  left off" list instead of leaving dead entries.
- The canvas no longer loses an edit made in the last couple of seconds before
  you navigate away: the final save now compares against what the backend
  actually stored, not against a save that was only scheduled.
- Opening a canvas from a hand-edited or foreign bundle drops any malformed
  stroke instead of letting a bad point crash the view.
- Renaming a note now saves the new title if you leave the page immediately, and
  editing a note refreshes the knowledge graph so new links appear without a
  reload.
- Renaming a notebook no longer loses the new name if you leave the page without
  clicking away from the field first (for example by pressing Ctrl+N to start a
  note); the rename is flushed on the way out, matching the note editor.
- A new text label typed on the canvas is included in the save made on the way
  out, so it is no longer dropped if you navigate away before confirming it.
- Closing the command palette returns focus to whatever opened it, so keyboard
  users are not dropped to the top of the page.
- Regenerating an audio overview during playback stops the old audio instead of
  leaving it reading over mismatched highlights.

- Word export kept the numbers on numbered lists and now emits emoji and other
  characters beyond the basic range as valid document text.
- The Connections graph no longer counts a phantom link after a linked note is
  deleted; a note's connection count always matches the lines drawn.
- Prompt Studio ignores an empty rewrite and never lets a slow refinement land
  on top of an edit you made while it was running.

## [0.3.0] - 2026-07-12

### Added

- Prompt Studio: build a clear prompt from simple parts (role, task, context,
  format, tone, constraints, examples) with a live preview, then sharpen it
  with the active model. It teaches prompt structure by showing it.
- Connections: a calm diagram of how the notes in a notebook link to each
  other, with an accessible list of the most-connected notes.
- Document outline: a navigable tree of a document's sections, built from its
  real heading structure, that jumps to any passage.
- Word export: save a note as an RTF document that opens in Word, Pages, or
  LibreOffice with real formatting, alongside the existing Markdown export.
- Command palette: Ctrl+K opens one box to jump to any page or notebook, or
  run an action, fully keyboard driven.
- Drag and drop import: drop PDF, TXT, or Markdown files anywhere on the
  window to import them, with a live drop target.
- "Pick up where you left off": the notebooks page surfaces your three most
  recently edited notes across all notebooks.
- Regenerate: ask the same question again on the latest chat answer.
- Note export to Markdown from the editor.
- Live word count in the editor.
- Rename a notebook by editing its title in place.
- Copy button on every chat answer.

### Fixed

- Auto-update actually runs now. The updater plugin was registered but never
  invoked, so no install ever checked for or applied updates. The app now
  checks on launch, downloads in the background, and the status bar offers a
  one-click restart when a new version is staged.

### Changed

- Removed the last unwired backend commands; every registered command now has
  a real caller in the interface, the REST API, or the system layer.
- README leads with a branded hero illustration of the cited-answer flow.

## [0.2.0] - 2026-07-12

### Fixed

**Critical**
- IPC argument casing mismatch that broke chat, search, document import and
  listing, notes, transforms, thinking partner, and podcasts at runtime.
  All commands now use snake_case arguments, enforced by tests on both sides.
- Long AI calls ran on the main thread and froze the whole app for up to two
  minutes. Chat, transforms, thinking partner, podcasts, search, and import
  are now async on worker threads.
- Installers shipped `llama-server` without the shared libraries it loads, so
  the bundled local AI could never start. Libraries are now downloaded,
  checksum verified, bundled, and found at runtime on all three platforms.
- Auto-update was dead end to end: update artifacts were never generated and
  the updater endpoint pointed at a release that never resolved. Releases now
  produce signed update bundles plus `latest.json` and publish automatically.
- A crashed local server could never be restarted, stop left it marked as
  crashed, and quitting the app orphaned the llama-server process.
- Restarting the sidecar accumulated duplicate dead providers.
- Auto-detected providers registered a hardcoded model name instead of the
  model the server actually has loaded.
- The REST API token was generated and thrown away, so every documented
  endpoint returned 401 forever. The token now appears in Settings.
- Malformed PDFs could abort the entire app; imports now fail with an error.
- Podcast generation crashed on documents with non-ASCII text.
- Editor auto-save could resurrect stale content over newer edits, silently
  dropped edits made just before navigating away, and swallowed failures
  without any indication. There is now a save status indicator.
- The message you sent disappeared during the AI's thinking time, and errors
  discarded your draft. Messages now render immediately and failed drafts
  return to the input.
- Deleting a notebook relied on a dialog that silently never appears on
  macOS. All destructive actions now use in-app two-step confirmation.
- Notes could not be deleted from the interface at all.
- Search hid page 1 results' page numbers when the page number was zero.
- Podcast playback continued after leaving the page.
- The sample notebook resurrected after users deleted every notebook.

### Added

- Citations under every chat answer: source chips with document title,
  heading, page, and an expandable snippet, backed by real retrieval scores.
- Conversation history: resume, continue, and delete past chats.
- Semantic search: documents are embedded in the background after import and
  queries blend vector similarity with keyword ranking when available.
- Wiki-links now navigate: clicking `[[a note]]` opens it, creating it first
  if needed. A backlinks panel shows which notes link to the open one.
- Local AI server controls on the Models page: start, stop, restart after a
  crash, with live status.
- Keyboard shortcuts, for real this time: Ctrl+K search, Ctrl+N new note,
  Ctrl+S save now. The header search button works.
- "Check for providers" actually re-probes local endpoints.
- Local REST API section in Settings with the session token and a copy-ready
  curl command.
- Friendly error messages with recovery hints across every page.

### Changed

- Accessibility pass: every card and result reachable by keyboard, WCAG AA
  contrast in both themes, visible focus everywhere, labels on all form
  fields, screen reader announcements for chat, reduced motion support, and
  a rem-based type scale that honors system text size.
- Deeper, richer blue accent palette in both themes.
- Release pipeline: version guard, SHA256SUMS, automatic publishing, and all
  actions pinned to commit SHAs. Auto-update covers Windows and macOS; Linux
  updates ship through the .deb and .rpm packages.
- CI: rustfmt gate, frontend tests on all three platforms.
- Windows and macOS icons are now real multi-resolution assets.
- Sidecar downloads are verified against pinned SHA256 checksums.
- Removed the unused filesystem plugin, dead commands, and the dead model
  registry to shrink the attack surface.

## [0.1.0] - 2026-04-06

### Added

**Core**
- Tauri v2 desktop application (Windows, macOS, Linux)
- SQLite database with WAL mode, foreign keys, and FTS5 full-text search
- 34 Tauri IPC commands across 9 modules
- REST API server on localhost:8484 for external automation
- CI/CD pipeline with GitHub Actions (lint, typecheck, cargo check, clippy)
- Cross-platform release workflow (Windows .msi, macOS .dmg, Linux .AppImage/.deb)
- Dependabot for npm, Cargo, and GitHub Actions dependency updates

**Documents**
- Import PDF, TXT, and Markdown files
- Paragraph-aware text chunking with overlap for RAG retrieval
- SHA-256 file deduplication
- PDF page extraction with heading detection
- Markdown frontmatter stripping and heading extraction
- File size guard (50MB max)

**Editor**
- Milkdown WYSIWYG Markdown editor with GFM support
- Wiki-link `[[...]]` decoration plugin with styled inline marks
- Debounced auto-save (2 second interval) with unmount cleanup
- Title editing with blur-save

**AI Features**
- RAG chat with citations (3-phase lock-free pipeline)
- Multi-provider AI support (Ollama, llama.cpp, LM Studio, OpenAI-compatible)
- Thinking Partner: mind map generation from documents
- Thinking Partner: Socratic questioning mode
- Content transformations: summarize, extract key points, custom prompts
- FTS5 search with BM25 relevance ranking (LIKE fallback)
- Prompt injection defense in all 4 LLM prompt templates

**UI**
- 10 frontend pages (Notebooks, Detail, Editor, Search, Chat, Think, Transform, Models, Settings, Podcasts)
- Dark and light themes with CSS custom properties
- Design system: Play + Source Serif 4 + JetBrains Mono typography
- Color palette: #EFECE3, #8FABD4, #4A70A9, #000000
- Dynamic status bar (active provider + indexed chunk count)
- First-run sample notebook with 2 getting-started notes
- Zustand store for active notebook context (persisted to localStorage)

**Security**
- Content Security Policy (CSP) with restrictive defaults
- Tauri capabilities scoped to app-specific directory
- SSRF validation on provider URLs (scheme, private IP blocklist, loopback restriction)
- HTTPS enforcement for API key transmission to cloud providers
- Error message sanitization (no internal details leaked to frontend)
- Path canonicalization on file imports
- All SQL queries parameterized (zero injection vectors)

### Known gaps in 0.1.0 (closed in 0.2.0)
- AI podcasts, semantic vector search, and the auto-updater shipped after
  this release; see 0.2.0.
