# C API

The library exports C-compatible entrypoints for every export flow that is also
available from the CLI.

All functions return a heap-allocated UTF-8 C string (`const char*`):

* On success: the generated output (JSON/text/notation/YAML).
* On failure: an error message.

Release every returned pointer with `free_c_string`.

## Exported functions

| C function | CLI equivalent |
|---|---|
| `create_presentation_from_file_c` | `presentation` |
| `create_text_from_file_c` | `text` |
| `create_lilypond_from_file_c` | `lilypond` |
| `create_abc_from_file_c` | `abc` |
| `create_song_yml_from_file_c` | `song-yml` |
| `get_song_from_file_as_json_c` | importer/parsing entrypoint |
| `free_c_string` | memory cleanup helper |

## Notes

* String parameters must be valid UTF-8.
* Optional text parameters (`template`, `language`, `separator`) can be `NULL`
  or empty.
* Boolean-like integer flags use `1` = true and any other value = false.
