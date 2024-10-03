
# BundleRepo <!-- omit in toc -->

**BundleRepo** is a beta tool designed to clone and pack a local or remote
(GitHub only for now) Git repository into a comprehensive XML file. The packed
XML includes detailed metadata about each file, such as the size in bytes and
the number of lines, making it suitable for large language model (LLM)
consumption, code analysis, and repository review.

XML was chosen for the file output format since it is very well structured and
LLM models can easily parse it (better than a plain-text dump).

- [Features](#features)
- [Usage](#usage)
  - [Installation](#installation)
  - [Running the Tool](#running-the-tool)
  - [Output File](#output-file)
  - [Choose Model for Token Count](#choose-model-for-token-count)
  - [GitHub Token](#github-token)
- [Help](#help)
- [Planned Improvements](#planned-improvements)
- [XML Layout](#xml-layout)
- [Beta Status](#beta-status)
- [Acknowledgements](#acknowledgements)
- [License](#license)

## Features

- **Clone Git Repositories**: Supports cloning both public and private
  repositories (with token support). Only supports `https` URLs at this time.
- **File Scanning**: Automatically scans the repository and lists all files,
  excluding standard ignored files (e.g., `.gitignore`, `LICENSE`).
- **Metadata Extraction**: For each file, the XML output includes:
  - `path`: the file path relative to the repository root
  - `size`: file size in bytes
  - `lines`: number of lines in the file
  - Raw file content (not escaped)
- **XML Output**: Generates an XML file (`packed-repo.xml`) that contains the
  entire repository structure and file details.

## Usage

This will be available as a binary and from `Crates.io` in the future, but for
now, you can build it from source. You will need to have
[Rust](https://www.rust-lang.org/tools/install) installed on your system to
build the project.

### Installation

1. Clone the project and install dependencies.

    ```bash
    git clone https://github.com/seapagan/bundle-repo.git
    cd bundle-repo
    cargo build --release
    ```

2. Move the binary to a directory in your `PATH`:

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

Or use the current directory:

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

### Output File

By default, the XML output will be written to `packed-repo.xml`, which contains
the hierarchical structure and metadata of the repository files. This can
then be passed to an LLM model for analysis (for example, attach the output
file to a ChatGPT or Claude prompt). The filename can be changed using the
`--file` or `-f` flag:

```bash
bundlerepo user_name/repo_name --file my-repo.xml
```

The output file will be written to the current directory unless a path is
specified:

```bash
bundlerepo user_name/repo_name --file /path/to/output.xml
```

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

> [!NOTE]
>
> This is totally optional if you are only using public repositories.

## Help

For help and additional options, you can run:

```bash
bundlerepo --help
```

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

This tool is currently in **beta**. While the core functionality works, there
may be edge cases or features yet to be fully refined. Feedback and
contributions are welcome to improve and stabilize the tool.

There is a pressing need for a test suite to ensure the tool works as expected
in a variety of scenarios. This is a priority for the next release.

## Acknowledgements

**Bundle Repo** is a rewrite of the original
[Repopack](https://github.com/yamadashy/repopack) project. The generated XML
metadata and structure are inspired by the output of Repopack, with enhancements
that include additional file attributes, instructions for the LLM and a more
robust structure. At this time `xml` output is the only supported output format,
however future versions may include additional formats. XML was chosen since it
is very well structured and LLM models can easily parse it (better than a
plain-text dump).

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
