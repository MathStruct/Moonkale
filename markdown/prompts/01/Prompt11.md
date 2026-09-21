Certain Language servers like these of Julia or Lean in VSCode allow for the posibility of writing Unicode symbols:

by this an escape sequence is chosen (typically "\" ) and then the name of the unicode character is typed in. Once typing starts a dropdown menu appears which 
Like:
 - \int yields: ∫ 
 - \:ladybug: yields: 🐞
 - \alpha yields: α


I would like this behavior to be default for the whole editor, that similar to writing Chinese with pinyin relacement by unicode characters is possible. (I assume Chinese is implemented by OS, so don't bother with that.)

Assume that one unicode character can have multiple names like \int and \integral for ∫

Maybe for the beginning orient yourself around this: https://docs.julialang.org/en/v1/manual/unicode-input/
Although make that list somehow editable/extendable.

Don't implement, just add it to the documentation and plan.