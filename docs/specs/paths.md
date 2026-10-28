# Paths

Note ids, what counts as a note, a book and a folder, how a title becomes a
file name, and vault names. Code: `crates/notes-store/src/note_id.rs`,
`names.rs`, `crates/notes-core/src/vault.rs`, `new_note.rs`, `rename.rs`. The
paths that sync accepts follow other rules (`docs/specs/sync.md`, SYNC-9).

Next id: PATH-23

## Note ids

- **PATH-1** A note id is the path of a note from the vault root, without
  `.typ`, with `/` as the separator: `Network/SSH`. The name of an id is its
  last segment (`SSH`), its folder is everything before the last `/` (`Network`,
  empty at the root).
- **PATH-2** An id is refused when it is empty, starts or ends with `/`,
  contains `\`, or has an empty segment, a `.` segment or a `..` segment.
- **PATH-3** An id is refused when any of its segments starts with `_` or `.`:
  such names are internal (`_baluk`, `.git`, `_folder.toml`).
- **PATH-4** An id is refused when its last segment ends with `.typ`
  (`SSH.typ`). Only the exact lowercase extension counts: `SSH.TYP` and
  `a.typ.b` are valid ids.
- **PATH-5** Any other character is allowed in an id, including spaces,
  non-ASCII letters and the characters Windows forbids in file names (`:`, `*`,
  `?`, `"`, `<`, `>`, `|`). Ids are compared character by character: names that
  differ only in case or in Unicode normalization form are different ids.

## Notes, books and folders

- **PATH-6** A `.typ` file outside a book is a note; its id is its path without
  `.typ` (`Сеть/SSH.typ` is `Сеть/SSH`).
- **PATH-7** A folder that holds a `main.typ` is a book; its id is the folder's
  path. Every other file inside it, at any depth, is a chapter or a resource of
  the book, not a note. Books do not nest: the book closest to the root owns
  everything below it.
- **PATH-8** A `.typ` file whose path without `.typ` is not a valid id is not a
  note and is left out of every list, without an error: a name starting with
  `_` or `.` anywhere in the path (`_baluk/lib.typ`, `Заметки/_черновик.typ`),
  or a file `a.typ.typ`. A file whose extension is not exactly `typ` (`a.TYP`)
  is not a source file at all.
- **PATH-9** A directory is a vault folder when it is not a book or inside a
  book, its path is a valid id, and it either holds no files (files with
  internal names, starting with `_` or `.`, do not count) or holds a `.typ`
  file somewhere below it. The folders on the way to it are folders too. A
  directory with only non-note files (`img/`, `code/` next to a note) is not
  a folder.

## From a title to a file name

Creating a note with a title (`notes new --title`) and renaming derive the file
name (for a book, the folder name) from the title with these rules. The title
itself is kept in the file exactly as typed, with every character.

- **PATH-10** A blank title is refused ("empty title") when creating or
  renaming. Creating a note is also refused when its path already holds a note
  or a folder, or lies inside a book (a book holds chapters, not notes).
- **PATH-11** The name is made from the title in Unicode NFC: a letter typed as
  a base character plus a combining mark becomes the single composed character.
- **PATH-12** The characters `/ \ : * ? " < > |` and control characters become
  spaces, runs of whitespace collapse to one space, and the ends are trimmed.
  A `/` in a title therefore never makes a folder.
- **PATH-13** `_`, `.` and spaces are removed from the start of the name, `.`
  and spaces from its end, and a trailing `.typ` (any case) is removed; this
  repeats until nothing changes. A dot inside the name stays (`v1.2 релиз`).
- **PATH-14** The name is cut to at most 80 characters and at most 200 bytes,
  by whole characters, and its ends are cleaned again after the cut.
- **PATH-15** When the part of the name before its first dot is, ignoring case,
  a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to
  `LPT9`) or `main`, a `_` is added after that part: `CON_`, `con_.txt`,
  `Main_`. Names that only start with such a word (`CONSOLE`, `COM10`,
  `main идея`) are kept.
- **PATH-16** When nothing is left, the name is `Без названия`.
- **PATH-17** The result is always a valid note id (PATH-2 to PATH-4).
- **PATH-18** A name already taken in the folder gets a number: `Name 2`,
  `Name 3`, up to 1000, after which the creation is refused. A name is taken by
  a note, a book or a folder with the same name, compared ignoring case and
  Unicode normalization form (a name stored decomposed counts). The folder part
  of the new id keeps the form in which the folder was given.
- **PATH-19** Renaming derives the new name from the new title by the same
  rules; the renamed item itself does not count as a taken name. Renaming a
  folder that is a book is refused.

## Vault names

- **PATH-20** A vault name is also the name of its folder. It is refused when
  it is blank, has spaces at the start or the end, is longer than 64
  characters, starts with `_` or `.`, ends with `.`, or contains a control
  character or any of `/ \ : * ? " < > |`. Spaces inside, non-ASCII letters
  and other punctuation are allowed.
- **PATH-21** The vaults of the data directory are the folders of `vaults/`
  with a valid name, sorted by name. A file there, and a folder starting with
  `_` or `.`, is not a vault.
- **PATH-22** Creating a vault whose folder already exists is refused
  ("exists"). Renaming a vault moves its folder with everything in it (notes
  and the vault's own settings) and is refused when the new name is invalid or
  belongs to another vault. Deleting a vault moves its whole folder to the
  system trash.
