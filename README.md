# Beyond Ctrl-C: The dark corners of Unix signal handling

These are **supplementary materials** for Rain's RustConf 2023 talk on signal handling with Rust.

## Links

### The presentation

* [Slides](https://docs.google.com/presentation/d/e/2PACX-1vQuV0pYyWQulFbnRHfgvtHqQ_ZVyrnkmfQxT0eQfjEpVjnptUIG8uMx9FuUDz3wxtWqVN5QA9C4biZT/pub?start=false&loop=false&delayms=3000), including a bonus section on SIGPIPE that was left out of the talk.
* Once the presentation is uploaded to YouTube it will be linked here.

### Personal links

* [My website](https://sunshowers.io/)
* **Mastodon:** [@rain@hachyderm.io](https://hachyderm.io/@rain)
* **Bluesky:** [@sunshowers.io](https://bsky.app/profile/sunshowers.io) --- I'm less active there but happy to answer questions if you tag me there
* I do not have an active "X"/Twitter account.
* Email me at rustconf2023 at sunshowers.io.

### Other links

* Most of the content of the talk comes from me learning about how to use signals with
  [cargo-nextest](https://nexte.st/). Nextest is a next-generation test runner for Rust with much
  faster test runs and many other features. Try it out!
* I'm the maintainer of [a number of Rust crates](https://crates.io/users/sunshowers). If the
  standard library's `Path::to_str` and `Path::display` have ever annoyed you, check out
  [camino](https://crates.io/crates/camino/) for UTF-8 paths. (The download manager demo uses camino
  for a smoother experience.)
* Doug McIlroy, the creator of Unix pipes, made an insightful [post about signals](https://www.tuhs.org/pipermail/tuhs/2015-September/007509.html) in 2015.
  
  > Signal() was there first and foremost to support SIGKILL; it
did not purport to provide a sound basis for asynchronous IPC.
The complexity of sigaction() is evidence that asynchrony remains
untamed 40 years on.

## Contents

### download-manager

The `download-manager` directory contains a simple download manager, used as an example throughout
the presentation. Run it from the root directory with `cargo run -p download-manager -- download
download-manifest.toml`. This will kick off downloads for two Linux ISOs (see
`download-manifest.toml`) into the `out/` directory.

Try pressing Ctrl-C while the downloads are happening! You should see the signal handler kick in,
and log entries saying that the downloads have been marked as interrupted.

There are a number of exercises included in the source code -- search for `TODO/exercise` and try
them out! Feel free to create forks with solutions, but please **do not send pull requests** with
solutions.

## Contributing

Pull requests fixing typos or clarifying content are welcome. Please do not send PRs containing demo
solutions.

## License

The code in this repository was originally written by me (Rain). Copyright has been waived to the
greatest extent possible, as specified by
[CC0-1.0](https://creativecommons.org/share-your-work/public-domain/cc0/). (However, note that if
you distribute a version of the download-manager binary, the licenses of the upstream crates used
will prevail.)
