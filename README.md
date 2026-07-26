# PRONIUM

**License**: [BSD 3-Clause License](LICENSE.md)

---

## 🇬🇧 English

PRONIUM is an operating system built in Rust. This guide describes how to build and run the PRONIUM OS.

### Prerequisites

To build and run PRONIUM, you will need the following tools installed on your system:
- **Rust and Cargo** (via `rustup`): For compiling the kernel. The project requires the `nightly` toolchain and the `rust-src` component. You can set it up with:
  ```bash
  rustup toolchain install nightly
  rustup default nightly
  rustup component add rust-src
  ```
- **xorriso**: To generate the bootable ISO image.
- **QEMU** (`qemu-system-x86_64`): To run and test the operating system in a virtual machine.
- **dosfstools** (`mkfs.fat`): To create the FAT32 virtual drive used by the OS.
- **Bash**: To execute the build scripts.

### Build and Run

The easiest way to build the project, create the ISO, and launch it in QEMU is to use the provided shell script:

```bash
./build_and_run.sh
```

This script will automatically:
1. Compile the kernel (`pronium_os`) for the `x86_64-unknown-none` target.
2. Prepare the `iso_root` directory with the kernel and the Limine bootloader files.
3. Generate a bootable `image.iso` file using `xorriso`.
4. Create a 64MB FAT32 virtual drive image (`fat32.img`) if it doesn't already exist.
5. Launch the OS in QEMU.

### Manual Build

If you only want to build the kernel without running the emulator:

```bash
cargo build -p pronium_os --target x86_64-unknown-none
```

To build just the ISO, you can run:

```bash
./build_iso.sh
```

---

## 🇷🇺 Русский

PRONIUM — это операционная система, написанная на Rust. В этом руководстве описан процесс сборки и запуска ОС PRONIUM.

### Требования

Для сборки и запуска PRONIUM в вашей системе должны быть установлены следующие инструменты:
- **Rust и Cargo** (через `rustup`): Для компиляции ядра. Проекту требуется `nightly`-версия компилятора и компонент `rust-src`. Установить их можно командами:
  ```bash
  rustup toolchain install nightly
  rustup default nightly
  rustup component add rust-src
  ```
- **xorriso**: Для создания загрузочного ISO-образа.
- **QEMU** (`qemu-system-x86_64`): Для запуска и тестирования ОС в виртуальной машине.
- **dosfstools** (`mkfs.fat`): Для создания виртуального диска FAT32.
- **Bash**: Для выполнения скриптов сборки.

### Сборка и запуск

Самый простой способ собрать проект, создать ISO-образ и запустить его в QEMU — использовать готовый bash-скрипт:

```bash
./build_and_run.sh
```

Этот скрипт автоматически:
1. Скомпилирует ядро (`pronium_os`) под целевую архитектуру `x86_64-unknown-none`.
2. Подготовит папку `iso_root` с ядром и файлами загрузчика Limine.
3. Сгенерирует загрузочный файл `image.iso` с помощью `xorriso`.
4. Создаст образ виртуального диска FAT32 размером 64 МБ (`fat32.img`), если его еще нет.
5. Запустит операционную систему в эмуляторе QEMU.

### Ручная сборка

Если вы хотите только скомпилировать ядро без запуска эмулятора:

```bash
cargo build -p pronium_os --target x86_64-unknown-none
```

Чтобы собрать только ISO-образ, выполните:

```bash
./build_iso.sh
```
