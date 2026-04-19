# Rules

- Some quotes that I found interesting:

> you can always optimize code you understand, you can't debug code you don't

> Two lines of duplication = coincidence. Three = pattern. Abstract on three.

# Notes

## src/main.rs

- `lines()` is one allocation for string and one drop when last_line is overwritten
- `last_line` takes the ownership (copy pointer,length and capacity of allocated string)

