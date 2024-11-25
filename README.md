# BundleRepo <!-- omit in toc -->

**BundleRepo** is a beta tool designed to clone and pack a local or remote
(GitHub only for now) Git repository into a comprehensive XML file. The packed
XML includes detailed metadata about each file, such as the size in bytes and
the number of lines, making it suitable for large language model (LLM)
consumption, code analysis, and repository review.

XML was chosen for the file output format since it is very well structured and
LLM models can easily parse it (better than a plain-text dump).

It is inspired by [Repopack](#acknowledgements) which is a great tool, but is
written in TypeScript and needs a Node.js environment to run. Eventually this
project will produce binaries and not need Rust installed to run.

The generated XML metadata and structure are inspired by the output of Repopack
(a lot of the header text was taken from there), with enhancements that include
additional file attributes, instructions for the LLM and a more robust
structure. At this time `xml` output is the only supported output format,
however future versions may include additional formats.

> [!TIP]
>
> XML was chosen as the default output format since it is very well
> structured and LLM models can easily parse it (better than a plain-text dump -
> see this [link][why-xml] from Anthropic as to why XML is a superior format for
> feeding context and instructions into an LLM).

```pre
BundleRepo Version 0.2.0, © 2024 Grant Ramsay <seapagan@gmail.com>

Pack a local or remote Git Repository to XML for LLM Consumption.

-> Found a git repository in the current directory: '/home/seapagan/data/work/own/bundle-repo' (branch: add-config-file)
-> Successfully wrote XML to 'packed-repo.xml'

Summary:
     Total Files processed:  13
 Total output size (bytes):  79068
      Token count (GPT-4o):  18766
```

- [Compatibility](#compatibility)
- [Features](#features)
- [Usage](#usage)
  - [Installation](#installation)
  - [Running the Tool](#running-the-tool)
    - [Specify the branch for a remote Git repository](#specify-the-branch-for-a-remote-git-repository)
  - [Output](#output)
    - [Output to File](#output-to-file)
    - [Output to stdout](#output-to-stdout)
    - [Copy to Clipboard](#copy-to-clipboard)
    - [Add line numbers](#add-line-numbers)
  - [Choose Model for Token Count](#choose-model-for-token-count)
  - [GitHub Token](#github-token)
- [Command Line Options](#command-line-options)
- [Configuration File](#configuration-file)
- [Ignored Files](#ignored-files)
- [Planned Improvements](#planned-improvements)
- [XML Layout](#xml-layout)
- [Beta Status](#beta-status)
- [Acknowledgements](#acknowledgements)
- [License](#license)

## Compatibility

The tool is designed and tested to work on Linux, MacOS, and Windows (Windows 10
and 11 tested).

## Features

- **Clone Git Repositories**: Supports cloning both public and private
  repositories (with token support). Only supports `https` URLs at this time.
- **File Scanning**: Automatically scans the repository and adds all files to
  the output, excluding standard ignored files (e.g. `.gitignore`, `LICENSE`,
  etc).

> [!NOTE]
>
> Any file listed in a `.gitignore` file will be excluded from the output and
> metadata.
>
> Binary file content will always be excluded, though they will be listed in
> the `<repository_structure>` node and a `<file>` node will be created in the
> XML to show that the file was excluded and why.
>
> See [Ignored Files](#ignored-files) for a full list of excluded files.

- **Metadata Extraction**: For each file, the XML output includes:
  - `path`: the file path relative to the repository root
  - `size`: file size in bytes
  - `lines`: number of lines in the file
  - Raw file content (not escaped)
- **Token Count**: Calculates the number of tokens in the final XML file, based
  on the specified model (default is GPT-4o). Only OpenAI models are supported
  at this time, though I may add support for others in the future.
- **XML Output**: Generates an XML file (`packed-repo.xml`) that contains the
  entire repository structure and file details.
- **Global and local configuration files**: Allows you to set default values
  globally and override them on a per-project basis. All settings can be
  further overridden by command line options.

This tool is currently under active development, and more features will be
implemented quickly. Please **star** this repository to stay updated on new
releases and features.

## Usage

This will be available as a binary download in the future, but for now, you can
build it from source or install from `crates.io`. You will need to have
[Rust](https://www.rust-lang.org/tools/install) installed on your system to
build the project.

### Installation

Clone the project and install dependencies.

- From [crates.io][crates-io-page]:

   ```bash
   cargo install bundle_repo
   ```

- From source:

   ```bash
   git clone https://github.com/seapagan/bundle-repo.git
   cd bundle-repo
   cargo build --release
   ```

  Move the resulting binary to a directory in your `PATH`:

  eg for Linux or MacOS:

  ```bash
  sudo mv ./target/release/bundlerepo /usr/local/bin
  ```

### Running the Tool

Use the GitHub short form:

```bash
bundlerepo user_name/repo_name
```

Use the full URL:

```bash
bundlerepo https://github.com/user_name/repo_name
```

Or use the current directory (if it is a git repository):

```bash
bundlerepo
```

> [!IMPORTANT]
>
> Only the `https` protocol is supported at this time. The tool will not yet work
> with `ssh` URLs (ie **not** `git@github.com:seapagan/bundle-repo.git`)

> [!NOTE]
>
> The tool will actually bundle **any** files in the current directory (unless
> they are in the hard-coded ignore list).
> This can probably be useful for bundling any related files that you wish to
> feed to an AI. However, you may need to edit the `<purpose>` and
> `<instructions>` nodes in the output XML. I may add a flag to make this easier
> in the future (`--not-code` or something).
>
> However, it still needs to be an actual git repository or the code will exit.
> I may add a flag to allow non-git repositories in the future.

#### Specify the branch for a remote Git repository

If you want to specify a branch for a remote repository, you can do so using the
`--branch` or `-b` flag:

```bash
bundlerepo user_name/repo_name --branch my_branch
```

Without this flag, the default branch will be used, which is usually `main` or
`master`.

> [!NOTE]
>
> The `--branch` option only works for **remote repositories**. It has no effect
> when bundling a local repository. If you want to bundle a local repository
> with a specific branch, you will need to check out that branch before running
> the tool.

### Output

#### Output to File

This is the default operation of the tool, the XML output will be written to
`packed-repo.xml`, which contains the hierarchical structure and metadata of the
repository files. This can then be passed to an LLM model for analysis (for
example, attach the output file to a ChatGPT or Claude prompt).

The filename can be changed using the `--file` or `-f` flag:

```bash
bundlerepo user_name/repo_name --file my-repo.xml
```

The output file will be written to the current directory unless a path is
specified:

```bash
bundlerepo user_name/repo_name --file /path/to/output.xml
```

#### Output to stdout

You can output the XML to the terminal by using the `--stdout` or `-s` flag:

```bash
bundlerepo user_name/repo_name --stdout
```

This will print the XML output to the terminal, which can then be redirected to
a file or piped to another application.

In this case, the `--file` flag is ignored and no file is written to disk.

#### Copy to Clipboard

You can copy the XML output to the clipboard by using the `--clipboard` or `-c`
flag:

```bash
bundlerepo user_name/repo_name --clipboard
```

This will copy the XML output to the clipboard, which can then be pasted into
another application or file, or indeed directly into an LLM prompt. Note that it
is likely to be a large amount of text, so ensure your clipboard can handle it.

In this case, the `--file` flag is ignored and no file is written to disk.

#### Add line numbers

If you want to add line numbers to the output, you can use the `--lnumbers` or
`-l` flag:

```bash
bundlerepo user_name/repo_name --lnumbers
```

This will add line numbers physically to each line in the output, which can be
useful for debugging or analysis. Note that this will increase the token count
of the output, so be aware of that when using it. Extra info for the LLM will
be added to the `<instructions>` node to explain the line numbers.

### Choose Model for Token Count

After generating the xml file, the tool gives a count of the number of tokens
in the file, to give you an idea of context usage and costs. By default it
calculates the number of tokens for the GPT-4o model, but you can specify
another model using the `--model` or `-m` flag:

```bash
bundlerepo user_name/repo_name --model gpt3.5
```

Valid models are `gpt4o`, `gpt4`, `gpt3.5`, `gpt3` and `gpt2`. It is important
to use the correct model, as the token count is vastly different between the 3
and 4 series models.

> [!NOTE]
>
> Only OpenAI models are supported at this time, since the code uses the
> `tiktoken` library from OpenAI to count the tokens. I may add support for
> other models in the future, if I can find a decent library that supports them.
>
> Currently, the count returned by this tool is identical to that returned by
> their [web app](https://platform.openai.com/tokenizer).

### GitHub Token

For **private repositories**, or to bypass usage restrictions, you can provide a
GitHub token to access the repository. You can create a token by following the
instructions
[here](https://docs.github.com/en/github/authenticating-to-github/creating-a-personal-access-token).

Once you have the token, you can pass it to the tool using the `--token` flag:

```bash
bundlerepo user_name/repo_name --token YOUR_GITHUB_TOKEN
```

> [!TIP]
>
> Passing a token is totally optional if you are only using public repositories.

## Command Line Options

The full list of command line options can be seen by running with the `--help`
flag:

```pre
Pack a local or remote Git Repository to XML for LLM Consumption.

Usage: bundlerepo [OPTIONS] [REPO]

Arguments:
  [REPO]  GitHub repository to clone (e.g. 'user/repo' or full GitHub URL). If not provided, the current directory will be searched for a Git repository.

Options:
  -b, --branch <BRANCH>     Specify a branch to checkout for remote repositories
  -f, --file <OUTPUT_FILE>  Filename to save the bundle as. [default: packed-repo.xml]
  -s, --stdout              Output the XML directly to stdout without creating a file.
  -m, --model <MODEL>       Model to use for tokenization. Supported models: 'gpt4o', 'gpt4', 'gpt3.5', 'gpt3', 'gpt2' [default: gpt4o]
  -c, --clipboard           Copy the XML to the clipboard after creating it.
  -l, --lnumbers           Add line numbers to each code file in the output.
  -t, --token <TOKEN>       GitHub personal access token (required for private repos and to pass rate limits)
  -V, --version            Print version information and exit
  -h, --help               Print help
```

## Configuration File

The tool supports two configuration files:

- Global config at `~/.config/bundlerepo/config.toml`
- Local config at `.bundlerepo.toml` in your current directory

This allows you to set default values globally and override them on a
per-project basis. All settings can be further overridden by command line
options.

The configuration files use TOML format. Here's an example configuration:

```toml
# ~/.config/bundlerepo/config.toml or .bundlerepo.toml
output_file = "my-default-output.xml"
model = "gpt3.5"
stdout = false
clipboard = false
line_numbers = true
token = "your-github-token"
```

All settings are optional. Settings are applied in the following order of
precedence (highest to lowest):

1. Command line options
2. Local config file (`.bundlerepo.toml`)
3. Global config file (`~/.config/bundlerepo/config.toml`)
4. Built-in defaults

Available configuration options:

- `output_file`: Default output filename (default: "packed-repo.xml")
- `model`: Default model for token counting (default: "gpt4o")
- `stdout`: Whether to output to stdout by default (default: false)
- `clipboard`: Whether to copy to clipboard by default (default: false)
- `line_numbers`: Whether to add line numbers by default (default: false)
- `token`: Your GitHub personal access token (default: none)

> [!TIP]
>
> Storing your GitHub token in the configuration file can be more convenient
> than passing it via command line, especially if you frequently work with
> private repositories. Just be sure to keep your configuration file secure.

## Ignored Files

The tool will ignore the following files by default and (except for binary, see
below) they will not be listed anywhere in the XML output:

- **ANY Binary File**. If you have a binary file in your repository, it will be
  listed in the XML output, but the content will be excluded.
- `.gitignore`
- any file **listed** in a `.gitignore` file
- `.git` folder and it's contents
- `.github` folder and it's contents
- Python requirements files (`requirements.txt`, `requirements-dev.txt`, etc)
- Lockfiles - any file ending in `.lock`
- `renovate.json`
- `license` files (e.g. `LICENSE`, `LICENSE.md`, etc)
- `.vscode` folder and it's contents

This list is hard-coded (and to be honest is tuned to my current workflow) and
cannot be changed at this time. However, that will be changed once the
configuration file functionality is added.

> [!TIP]
>
> I'm very open to adding other files that should be ignored by default, If you
> have a suggestion, please open a PR or an Issue on GitHub. For example, tool
> configuration files (eslintrc, prettierrc, etc), which are not needed by an
> LLM and just take up token space.
>
> If there is demand, I may add a flag to allow the user to bypass this list and
> include all files. However, binary files will always be excluded as they don't
> fit well in XML.

## Planned Improvements

You can find planned improvements and known issues etc in the [TODO.md](TODO.md)
file.

## XML Layout

The generated `packed-repo.xml` follows a structured format that can be easily
understood by an LLM. Below is an example layout with explanations for each tag:

```xml
<repository>
  <file_summary>
    <!-- Metadata describing the purpose and file structure of the packed repository -->
    <!-- It also contains some instructions to help the LLM properly decode and understand the data -->
  </file_summary>

  <repository_structure>
    <summary>
      <!-- A brief summary of the folder structure in the repository -->
    </summary>
    <folder name="src">
      <!-- Folders contain nested folders and files -->
      <file path="main.rs">
        <!-- Files are listed by path relative to the repository root -->
      </file>
    </folder>
  </repository_structure>

  <repository_files>
    <summary>
      <!-- A summary of the files and their contents -->
    </summary>
    <file path="src/main.rs" size="1474" lines="53">
      <!-- For each file, the path, size in bytes, and number of lines are provided -->
      <!-- Full file contents are included here -->
    </file>
  </repository_files>
</repository>
```

## Beta Status

> [!WARNING]
>
> This tool is currently in **beta**. While the core functionality works, there
> may be edge cases or features yet to be fully refined. Feedback and
> contributions are welcome to improve and stabilize the tool.
>
> There is a pressing need for a test suite to ensure the tool works as expected
> in a variety of scenarios. This is a priority for the next release.

## Acknowledgements

**Bundle Repo** is a rewrite of the original
[Repopack](https://github.com/yamadashy/repopack) project, though none of the
source code was used or even looked at (the output file header however was
heavily borrowed from). The idea was to create a similar tool from scratch, with
a few enhancements and improvements. It's also part of my journey to learn Rust
and build useful tools for all.

## License

This project is licensed under the MIT License.

```pre
The MIT License (MIT)
Copyright (c) 2024 Grant Ramsay

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE
OR OTHER DEALINGS IN THE SOFTWARE.
```

[why-xml]: https://docs.anthropic.com/en/docs/build-with-claude/prompt-engineering/use-xml-tags
[crates-io-page]: https://crates.io/crates/bundle_repo
