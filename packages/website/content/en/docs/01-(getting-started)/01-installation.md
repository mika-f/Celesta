Download the Celesta package for your operating system from the release assets. Check the release notes for the supported OS versions and available architectures.

- [Download for macOS ↓](celesta:download/macos)
- [Download for Windows ↓](celesta:download/windows)

Looking for another version or architecture? [Browse all releases](celesta:download/all).

> [!NOTE]
> **Everything you need to run Celesta**
> The macOS and Windows packages include the React runtime, Node.js, and the FFmpeg media plugin that reads and writes video and audio. You do not need to install Rust, Node.js, pnpm, or FFmpeg separately to create and export videos.

## 1. Install the app

::::tabs[Installation platform]{default="macOS"}
:::tab[macOS]

1. Choose the `macos-arm64.dmg` package for Apple Silicon, or `macos-x64.dmg` for an Intel Mac. The full filename includes the release version.
2. Open the disk image and drag **Celesta** into **Applications**.
3. Eject the disk image, then open Celesta from Applications.

:::
:::tab[Windows]

Download the `windows-x64-setup.exe` installer, run it, and open Celesta from the Start menu. The full filename includes the release version.

Prefer a portable app? Download the `windows-x64.zip` package, extract the entire archive, and launch `Celesta.exe`. Keep the runtime directory and DLLs beside the executable.

:::
:::tab[Linux]

Use the [source-build instructions](/en/docs/build-from-source/) for Linux. Check the release assets for any additional platform packages provided with a release.

:::
::::

## 2. Create your project

Choose **File → Create New Project…**, or the toolbar’s **Create New Project…** button, and select a folder such as `Documents/my-video`. The folder picker can create a new folder. Celesta initializes it and opens `film.tsx`. Keep projects separate from the installed app. See [Create a project](/en/docs/create-project/) for the CLI equivalent and generated files.

## 3. Edit your first scene

Open `film.tsx` in your text editor, change the title or colors, and save. Celesta reloads the preview. Press `Space` to play. For animation, save the [React title example](/en/docs/react-compositions/) as `first-scene.tsx`, then choose **File → Open…** and select it.

The app includes React examples such as `title.tsx`. See [where to find them](/en/docs/examples/).

## Updating Celesta

Quit Celesta and install the newer release package. For portable Windows builds, extract the new version into its own folder and launch it from there. Keep your projects and media in your Documents directory so they stay separate from app updates.

Working on Celesta itself? See [Build from source](/en/docs/build-from-source/) for the developer setup.
