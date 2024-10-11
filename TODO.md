# Planned Improvements

- add more output formats - Text, Markdown, maybe others.
- add a test suite to ensure the tool works as expected in a variety of
  scenarios.
- add a configuration file to store the token and other settings, will probably
  be a TOML file. Allow local and global configuration files.
- allow the user to add extra file exclusions, or allow files that are excluded
  by default to be included.
- add the ability to check for updates and update the tool (or at least notify
  the user that an update is available and where to get).
- actually remove comments from the generated XML file. Perhaps add a flag to
  allow the user to choose whether to include comments or not. Looking at using
  'tree-sitter' to parse the code and remove comments. Need to think about
  needed comments - like for linters and such, we may not want them removed?
  perhaps start by removing block comments and all docstrings? Then again, it is
  always good to have comments in the returned code from the LLM. Needs more
  thought. **If we add this, remember to re-add the note on comment removal to
  the output file `<notes>` node.**
- allow to specify which branch of the repository to use, use the default
  branch if not specified. Local repositories should be able to have the same,
  but default to the checked-out branch.
- ensure that the tool works on Windows, Linux, and macOS. It does work great on
  all 3 at this current code state, but we need to develop a test suite and get
  the CI pipeline working to ensure that it continues to work on all 3.
- allow to work with non-git repositories (local only obviously).
- see if we can find a library to tokenize other models such as `Claude` or
  `Gemini`
- change file encoding to UTF-8 for included files, this is to ensure that the
  XML file is valid and can be read by other tools and specifically LLM's who
  generally prefer UTF-8.
- allow user to add custom metadata to the XML file, this could be used to
  store information about the repository, such as the name, description, extra
  instructions, etc. Would again be once the TOML file is implemented.
- ignore `dotfiles` by default, but allow the user to include them if they want.
- Add secret-checking to the tool, to ensure that no secrets are included in the
  output XML file. Hopefully this can be done with a library, but may need to
  write our own checks.
