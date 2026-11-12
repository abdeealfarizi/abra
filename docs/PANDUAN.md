# Panduan Pengguna ABRA

ABRA mengganti taskbar Windows 11 dengan pengalaman ala macOS Tahoe: **Dock**, **Notch (Island)**, **Menu Bar**, **Spotlight**,
**Apps**, **Mission Control**, **Stage Manager**, **Pusat Kontrol**, dan **Hot Corners**, semuanya bergaya Liquid Glass
dan ringan.

> Panduan ini berbahasa Indonesia. Ringkasan dalam bahasa Inggris, cara build, dan informasi pengembangan ada di
> [README](../README.md).

## Daftar isi

1. [Memasang dan memulai](#1-memasang-dan-memulai)
2. [Dock](#2-dock)
3. [Notch (Island)](#3-notch-island)
4. [Menu bar](#4-menu-bar)
5. [Pusat Kontrol](#5-pusat-kontrol)
6. [Spotlight](#6-spotlight)
7. [Apps](#7-apps)
8. [Mission Control](#8-mission-control)
9. [Stage Manager](#9-stage-manager)
10. [Hot Corners](#10-hot-corners)
11. [Pengaturan](#11-pengaturan)
12. [Pintasan keyboard](#12-pintasan-keyboard)
13. [Privasi dan keamanan](#13-privasi-dan-keamanan)
14. [Pemecahan masalah](#14-pemecahan-masalah)
15. [Batasan yang diketahui](#15-batasan-yang-diketahui)
16. [Mencopot pemasangan](#16-mencopot-pemasangan)

## 1. Memasang dan memulai

**Persyaratan:** Windows 11 (build 22000 atau lebih baru), x64.

**Cara A – Installer** (`ABRA-Setup-<versi>.exe`, bila tersedia di halaman Releases): jalankan, ikuti petunjuk. Tidak butuh hak
Administrator; ABRA dipasang untuk pengguna Anda saja.

**Cara B – Zip portabel** (`ABRA-<versi>-win-x64.zip`): ekstrak ke folder mana pun dan jalankan `abra.exe`. **Jaga `abra.exe` dan
`abra-guard.exe` tetap satu folder.**

> Berkas ABRA belum ditandatangani secara digital, jadi Windows SmartScreen mungkin menampilkan "Windows protected your PC".
> Pilih **More info → Run anyway** bila Anda memang mengunduhnya dari halaman resmi proyek.

Begitu `abra.exe` dijalankan, taskbar bawaan Windows disembunyikan dan dock muncul di bawah layar.

- **Keluar:** klik kanan ikon di dock atau ikon ABRA di area tray → **Keluar dari ABRA**. Taskbar dipulihkan otomatis.
- **Bila ABRA mati mendadak:** `abra-guard.exe` otomatis memulihkan taskbar (dan jendela yang sedang diparkir Stage Manager). Bila
  tidak, jalankan `abra.exe` lagi lalu keluar secara normal.
- **Buka pengaturan:** klik kanan dock → **Pengaturan ABRA…**, ikon roda gigi di dock, atau logo bulat di menu bar → **Pengaturan ABRA…**.
- **Mulai otomatis saat login:** Pengaturan → Umum → **Jalankan saat login Windows**.

Hanya satu ABRA yang berjalan; menjalankannya lagi hanya membangunkan yang sudah ada. Taskbar Windows tidak dimatikan selamanya: ia
hanya disembunyikan selama ABRA berjalan (bisa dimatikan di Pengaturan → Umum → **Sembunyikan taskbar Windows**).

## 2. Dock

- **Buka/aktifkan app:** klik ikon. Klik lagi pada app yang sedang di depan untuk meminimalkannya.
- **Tambah app:** seret berkas atau pintasan (`.exe`, `.lnk`) ke dock; atau klik kanan dock → **Tambah app ke dock…**; atau klik kanan
  app yang sedang berjalan → **Simpan di dock**. App Microsoft Store (termasuk WhatsApp) didukung.
- **Susun ulang:** seret ikon yang disematkan ke posisi lain.
- **Hapus:** klik kanan ikon → **Hapus dari dock**, atau seret ikon keluar dari dock.
- **Ganti ikon:** klik kanan → **Ubah ikon…** (png, jpg, ico, bmp, gif). **Kembalikan ikon asli** untuk membatalkan. Berlaku juga untuk
  **tombol Mulai** di ujung kiri dock.
- **Tombol Launchpad:** tepat setelah tombol Mulai ada ikon kisi titik berwarna seperti di macOS. Klik untuk membuka Apps; klik kanan → **Ubah ikon…** untuk menggantinya (atau `--set-icon launchpad <gambar>`).
- **Ikon seragam ala macOS:** semua ikon diberi badan squircle yang sama besar dengan bayangan lembut; ikon berbentuk bebas
  diletakkan di atas pelat putih. Matikan di Pengaturan → DOCK bila ingin ikon asli Windows.
- **Pratinjau jendela:** diamkan kursor di atas ikon app yang berjalan. Klik pratinjau untuk membuka jendelanya.
- **Zoom:** ikon membesar saat kursor lewat; ukuran, jarak, zoom, dan jarak dari tepi diatur di tab **Ukuran**.
- **Paket ikon (opsional):** letakkan folder berisi PNG ikon (mis. koleksi ikon macOS) di `%APPDATA%\ABRA\iconpack`. ABRA mencocokkan app Windows yang punya padanan jelas dengan nama berkas di folder itu: Explorer → `Finder`, Kalkulator → `Calculator`, Notepad → `TextEdit`, Task Manager → `Activity Monitor`, Terminal/CMD/PowerShell → `Terminal`, Pengaturan → `System Settings`, Microsoft Store → `App Store`, Mail, Calendar, Maps, Photos, Weather, dan sebagainya, serta tombol Apps → `Apps`. Browser dan app pihak ketiga tidak pernah diganti. Ikon dari paket tampil apa adanya (tanpa pelat dan gaya tambahan) di dock, Spotlight, dan Apps. Ikon yang Anda pilih manual lewat "Ubah ikon…" selalu menang. Matikan di Pengaturan → Tampilan → "Paket ikon". ABRA **tidak membawa** ikon apa pun; Anda memasang paketnya sendiri dan bertanggung jawab atas lisensinya.
- **Tumpukan Unduhan** (sebelum tempat sampah): klik untuk melihat 10 berkas terbaru di folder Unduhan dan membukanya, atau pilih "Buka folder Unduhan". Berkas yang masih diunduh disembunyikan. Bisa dimatikan di Pengaturan → Dock.
- **App terakhir dipakai:** tiga app yang baru saja Anda tutup (bukan sematan) tetap muncul di akhir daftar app agar mudah dibuka lagi; daftarnya disimpan di `recents.txt`. Bisa dimatikan di Pengaturan → Dock.
- **Tooltip** nama ikon berekor kecil yang menunjuk ke ikon.
- **Tempat sampah** ada di ujung kanan dock; klik kanan untuk membuka atau mengosongkannya (Windows tetap meminta konfirmasi).
- **Mode dock** (tetap, menimpa, sembunyi otomatis, sentuh tepi) ada di halaman **Dock**. Material permukaan (Liquid Glass atau Hitam buram) dan tema terang/gelap diatur sekali untuk semua permukaan di halaman **Tampilan**.
- Animasi minimize, buka, dan tutup jendela dapat dimatikan di Pengaturan → Umum; bayangan jendela aktif di Pengaturan → Jendela.

**Gaya ikon:** Pengaturan → Tampilan → *Gaya ikon* mengubah semua ikon dock, tombol Apps, dan grid Apps: **Gelap** menggelapkan latar ikon, **Bening** membuatnya abu-abu kaca tembus pandang, **Berwarna** memetakan warna ikon ke satu warna tint (Biru, Ungu, Merah Muda, Oranye, Hijau). Ikon yang Anda ganti sendiri tidak ikut berubah.

## 3. Notch (Island)

Pil di tengah atas layar. Materialnya (**Liquid Glass** atau **Hitam buram**, bawaan Hitam) dipilih sendiri lewat *Material notch* di Pengaturan → Tampilan, terpisah dari dock dan menu bar; teksnya selalu putih sehingga notch memakai varian gelap. Ia melebar saat kursor menempel (jika **Buka otomatis saat kursor di notch** aktif) atau saat diklik.
Isinya: media yang sedang diputar (sampul, judul, kontrol, dan visualizer), kalender, timer, dan kontrol cepat (Wi-Fi, Bluetooth,
volume, kecerahan). Saat panel terbuka, gulir mouse di atasnya untuk berpindah panel.

Saat menu bar aktif, notch dibiarkan hitam polos seperti MacBook, kecuali ada media yang diputar. Mode dan kebulatannya
diatur di tab **NOTCH**; bisa dimatikan di Pengaturan → Umum → **Aktifkan notch**.

## 4. Menu bar

Aktifkan di Pengaturan → Umum → **Aktifkan menu bar**.

**Sisi kiri**
- **Logo bulat ABRA:** Tentang PC Ini, Pengaturan Sistem, Microsoft Store, Pengaturan ABRA, Aplikasi, Mission Control, Paksa Keluar
  (membuka Task Manager), Tidur, Mulai Ulang, Matikan, Kunci Layar. Aksi daya meminta konfirmasi lebih dulu.
- **Nama app aktif** (tebal): Tentang, Sembunyikan, Sembunyikan Lainnya, Tampilkan Semua, Keluar.
- **Menu app:** bila app aktif memiliki menu Win32 klasik (File, Edit, dst.), menu aslinya ditampilkan dan bisa diklik. App tanpa menu
  klasik (browser, app Electron, Notepad baru) memakai menu standar (Berkas, Edit, Tampilan, Jendela, Bantuan) yang mengirim pintasan
  keyboard ke app. Setelah satu menu terbuka, gerakkan kursor ke judul lain untuk berpindah tanpa klik.

**Sisi kanan** (dari kanan ke kiri): jam (klik membuka Pusat Notifikasi Windows), **Pusat Kontrol**, **Spotlight**, Wi-Fi, baterai
(klik membuka pengaturan Windows terkait). Ada ikon Stage Manager saat fiturnya aktif.

**Latar menu bar** (Pengaturan → Menu Bar): **Transparan** (bawaan, tanpa latar), **Terang** (putih rata), atau **Gelap** (hitam rata).
Warna teks menyesuaikan wallpaper atau warna latar. Tema terang/gelap tidak mengubah menu bar.

**Nama app di kiri** mengikuti jendela yang terakhir Anda klik dengan mouse (juga saat jendela itu tidak berpindah fokus), dan jendela
depan bila fokus berpindah lewat keyboard atau Stage Manager.

Secara bawaan dock, menu bar, dan notch **selalu berada di atas semua jendela, termasuk app layar penuh**. Bila Anda lebih suka
mereka menyingkir saat layar penuh (video, game, F11), matikan Pengaturan → Umum → **Selalu di atas jendela layar penuh**.

## 5. Pusat Kontrol

Klik ikon dua sakelar di menu bar. Panel berisi modul kaca mengambang: Wi-Fi (dengan nama jaringan), Bluetooth, Berbagi Terdekat (padanan AirDrop), Sedang Diputar, Stage Manager, Mission Control, Mode Gelap, Tangkapan Layar, Fokus, serta slider **Suara** dan **Layar** (kecerahan). Klik modul untuk menyalakan/mematikan; seret slider untuk mengubah nilai, dan kenop kaca bening muncul selama diseret. Tombol bulat di ujung kanan slider membuka pengaturan suara atau layar Windows.

**Edit Kontrol…** membuka mode edit dan galeri **Cari Kontrol**:
- badge **−** menghapus modul; seret modul ke modul lain untuk menukar urutan; **Selesai** menyimpan.
- Galeri dikelompokkan per kategori (Konektivitas, Layar & Kecerahan, Suara, dan seterusnya) dengan kolom cari dan sidebar berikon warna. Klik kontrol untuk menambahkannya.
- **Klik kanan** modul (di mode edit) untuk mengubah ukuran: **Kecil** (bulat), **Sedang** (kapsul), atau **Besar** (2×2). Slider dan Sedang Diputar berukuran tetap.
- Fokus, Hotspot, Berbagi Terdekat, dan Mode Pesawat membuka halaman pengaturan Windows yang sesuai.

**HUD volume dan kecerahan:** saat Anda menekan tombol volume/kecerahan, kapsul kaca muncul di kanan atas (di bawah menu bar) berisi slider dengan ikon di kedua ujung; bisa diseret dan hilang sendiri.

## 6. Spotlight

`Ctrl+Space` atau ikon kaca pembesar di menu bar. Saat kosong, Spotlight tampil sebagai kapsul "Spotlight Search" dengan empat tombol bulat di kanan. Begitu Anda mengetik, ia berubah menjadi panel hasil.

**Mencari:** ketik nama aplikasi (fuzzy, inisial didukung), ekspresi hitung seperti `12*(3+4)`, nama berkas, atau nama aksi. Hasil dikelompokkan: kalkulator, aplikasi, aksi, berkas, lalu baris **Cari di web**. Chip **Aplikasi / Berkas Saya / Aksi** di bawah kolom input menyaring jenis hasil. Panah atas/bawah memilih, `Enter` membuka, `Esc` membersihkan input lalu menutup. Baris web membuka Google di browser Anda **hanya bila dipilih**; ABRA sendiri tidak mengirim apa pun lewat jaringan.

**Empat mode** (tombol bulat, atau `Ctrl+1` sampai `Ctrl+4`, yang berlaku di mana saja di Spotlight):

| Pintasan | Mode | Isi |
|---|---|---|
| `Ctrl+1` | **Apps** | Semua aplikasi terpasang (lihat bagian 7) |
| `Ctrl+2` | **Berkas Saya** | Mencari nama berkas dan folder di Desktop, Dokumen, Unduhan, Gambar, Video, Musik, dan OneDrive (sampai 5 tingkat, hasil teratas 20). Bukan pencarian seluruh disk |
| `Ctrl+3` | **Aksi** | Perintah cepat: Buat Catatan, Mulai Timer, Kirim Email, Ambil Tangkapan Layar, Kunci Layar, Tampilkan Desktop, Ganti Mode Gelap, Stage Manager, Mission Control, Kalkulator, Task Manager, Pengaturan, Hapus Riwayat Papan Klip |
| `Ctrl+4` | **Papan Klip** | Teks yang Anda salin dalam 8 jam terakhir, lengkap dengan jamnya. `Enter` menyalinnya kembali ke papan klip |

**Quick keys untuk Aksi:** tiap aksi bisa dijalankan dengan huruf singkat. Di mode Aksi, ketik `nn` lalu `Enter` untuk Buat Catatan. Pilih sebuah aksi lalu tekan `Tab` untuk mengisi atau mengganti quick key-nya (sampai 6 huruf/angka), `Enter` menyimpan, `Esc` membatalkan. Tersimpan di `actions.txt` pada folder konfigurasi.

**Riwayat Papan Klip** bisa dimatikan di Pengaturan → Umum → **Riwayat Papan Klip di Spotlight**. Riwayat hanya disimpan di memori (hilang saat ABRA ditutup), tidak pernah ditulis ke disk atau dikirim ke mana pun. Isi yang ditandai pengelola kata sandi diabaikan. Hanya teks.

## 7. Apps

Tombol **Apps** di dock (ikon kisi titik berwarna), `Ctrl+Shift+Space`, `Ctrl+1` di Spotlight, menu logo → Apps, atau hot corner. Panel kaca di tengah atas dengan judul "Applications", chip kategori (Semua, Produktivitas, Kreativitas, Hiburan, Utilitas, Sistem), dan grid 5 kolom berikon seragam ala macOS.

- **Cari:** langsung ketik; grid ikut menyaring.
- **Pindah:** panah (atas/bawah/kiri/kanan) atau arahkan mouse; gulir mouse menggeser baris. `Enter` atau klik membuka.
- **Tutup:** `Esc` (pertama membersihkan pencarian, lalu kembali ke Spotlight, lalu menutup), atau klik di luar panel.

## 8. Mission Control

`Ctrl+↑`, menu logo, Pusat Kontrol, atau hot corner. Semua jendela terbuka ditata dalam grid thumbnail hidup; judul muncul saat kursor
di atas jendela. Bar di atas menampilkan Virtual Desktop (klik untuk berpindah, **+** untuk membuat desktop baru). Klik jendela untuk
mengaktifkannya; `Esc` atau klik area kosong untuk keluar. Jendela yang diminimalkan tidak ikut ditampilkan.

## 9. Stage Manager

Nyalakan di Pengaturan → **Stage** → **Aktifkan Stage Manager**, lewat Pusat Kontrol, atau lewat hot corner.

App yang sedang Anda pakai berada di "panggung". Jendela app lain **diparkir di luar layar** (bukan diminimalkan) dan tampil sebagai
kartu miring di strip samping layar, lengkap dengan lencana ikon. **Kartu itu hidup:** isinya adalah tangkapan jendela asli yang diperbarui terus (±20 kali per detik), jadi video YouTube di Brave/Chrome, animasi, atau jam tetap berjalan di dalam kartu.

- **Klik kartu:** menukar grup; jendela terbang dari kartu ke posisinya.
- **Shift-klik kartu:** menggabungkan app itu ke grup panggung sehingga satu panggung berisi beberapa app.
- **Strip menyingkir** bila jendela panggung menimpanya; sentuh tepi layar di sisi strip untuk memunculkannya sementara.
- **Klik wallpaper:** semua jendela masuk strip dan desktop terlihat.
- **Alt+Tab** diganti pengalih app ABRA (kartu jendela dengan sorotan biru). `Tab`/`Shift+Tab` atau panah memilih, `Esc` membatalkan,
  lepas `Alt` untuk berpindah. Bisa dimatikan di Pengaturan → Stage → **Ganti Alt+Tab dengan pengalih ABRA**.
- **Dimatikan atau ABRA keluar:** semua jendela dikembalikan ke posisi semula. Bila ABRA mati mendadak, jendela dipulihkan oleh
  `abra-guard` atau saat ABRA dijalankan lagi.

**Pengaturan Stage Manager** (tab **Stage**):
- **Posisi daftar jendela:** kiri atau kanan layar.
- **Ukuran kartu**, **jarak antar kartu**, **jumlah kartu** (grup yang melebihi batas dikembalikan ke layar), **kecepatan animasi** (50–200%).
- **Judul jendela saat hover:** nama app muncul di atas kartu yang disorot.
- **Klik wallpaper menyimpan semua jendela:** dapat dimatikan.
- **Jeda media app di latar:** media (browser, pemutar musik) milik app yang keluar panggung dijeda lalu dilanjutkan saat app kembali.
  Hanya untuk app yang mendaftarkan sesi media Windows.
- **Ikon desktop:** sembunyi, otomatis (tampil saat panggung kosong), atau tampil.
- **Program dikecualikan:** tulis potongan jalur atau nama exe (satu per baris, huruf kecil) di `stage_exclude.txt` pada folder
  konfigurasi ABRA (lihat [Pengaturan](#11-pengaturan)). Jendela program itu tidak pernah diparkir.

App yang berjalan sebagai Administrator tidak bisa dipindah oleh proses biasa, jadi Stage Manager membiarkannya apa adanya.

## 10. Hot Corners

Pengaturan → Jendela → **Hot Corners**. Klik tombol tiap sudut untuk berputar antar aksi: Tidak ada, Mission Control, Apps,
Tampilkan Desktop, Stage Manager, Pusat Notifikasi, Kunci Layar. Aksi berjalan setelah kursor diam di sudut sekitar 0,25 detik dan
diabaikan saat tombol mouse ditekan, jadi aman saat menyeret jendela.

## 11. Pengaturan

Jendela Pengaturan ABRA meniru Pengaturan Sistem macOS Tahoe: sidebar berikon warna di kiri dan halaman berisi kartu grup di kanan (gulir dengan roda mouse bila isinya panjang). Lampu merah di kiri atas menutup jendela; judul bisa diseret.

| Halaman | Isi |
|---|---|
| **Umum** | Bahasa (Indonesia/English), jalankan saat login, sembunyikan taskbar Windows, sembunyikan ikon desktop, selalu di atas jendela layar penuh, animasi jendela, bayangan jendela aktif |
| **Dock** | Aktifkan dock, perilaku, ikon seragam ala macOS, ukuran ikon, jarak, zoom, jarak dari tepi, kebulatan sudut |
| **Notch** | Aktifkan notch, perilaku, kebulatan sudut, buka otomatis saat kursor di notch |
| **Menu Bar** | Aktifkan menu bar, latar (Transparan, Terang, Gelap), jam 24 jam |
| **Tampilan** | Tema, **material** (Liquid Glass atau Hitam buram, berlaku global), gaya blur, tingkat blur, transparansi, refraksi tepi kaca, gaya ikon (Bawaan, Gelap, Bening, Berwarna) dan warna tint, paket ikon |
| **Stage Manager** | Semua pengaturan Stage Manager |
| **Sudut Layar** | Aksi untuk empat hot corner |
| **Spotlight** | Riwayat Papan Klip |

Semua perubahan langsung berlaku. Berkas konfigurasi ada di folder `%APPDATA%\ABRA`:

| Berkas | Isi |
|---|---|
| `settings.cfg` | Pengaturan (teks `kunci=nilai`) |
| `pins.txt` | App yang disematkan di dock |
| `icons.txt` dan folder `icons\` | Ikon kustom |
| `cc_layout.txt` | Susunan modul Pusat Kontrol |
| `stage_exclude.txt` | Program yang dikecualikan dari Stage Manager |
| `abra.log` | Log (berguna untuk laporan bug) |

**Opsi baris perintah** yang berguna untuk pengguna: `--keep-taskbar` (jangan sembunyikan taskbar Windows), `--open-settings`,
`--add-app <jalur>`, dan `--set-icon <app|start> <gambar>`. Variabel lingkungan `NACRE_CONFIG_DIR` memakai folder konfigurasi lain
(sekaligus menjadi instance terpisah).

## 12. Pintasan keyboard

| Pintasan | Aksi |
|---|---|
| `Ctrl+Space` | Spotlight |
| `Ctrl+Shift+Space` | Apps |
| `Ctrl+↑` | Mission Control |
| `Alt+Tab` | Pengalih ABRA (hanya saat Stage Manager aktif) |
| `` Alt+` `` / `` Alt+Shift+` `` | Pindah ke jendela berikut / sebelumnya dari app yang sama |
| `Ctrl+Alt+H` | Sembunyikan (minimalkan) semua jendela app aktif |
| `Ctrl+Alt+M` | Minimalkan jendela aktif |
| `Ctrl+Alt+Q` | Tutup semua jendela app aktif dengan sopan (app boleh bertanya "simpan?") |
| `Esc` | Menutup overlay aktif |

Pintasan lain (hot corner) diatur di Pengaturan. Bila sebuah pintasan tidak bekerja, mungkin sudah dipakai program lain (tercatat di
`abra.log`).

## 13. Privasi dan keamanan

- **Tanpa jaringan dan tanpa telemetri.** ABRA tidak mengirim data ke mana pun. Satu-satunya akses internet tidak langsung adalah
  baris "Cari di web" di Spotlight, yang membuka browser Anda hanya jika Anda memilihnya.
- **Data lokal.** Pengaturan, sematan, dan log disimpan di `%APPDATA%\ABRA`. Tidak ada data yang dikumpulkan.
- **Hook keyboard.** Saat Stage Manager dan pengalih Alt+Tab aktif, ABRA memasang hook keyboard tingkat rendah semata-mata untuk
  menelan kombinasi `Alt+Tab` dan menampilkan pengalihnya. Ketikan tidak dicatat, disimpan, atau dikirim. Hook dilepas saat fitur
  dimatikan.
- **Hook mouse** dipakai singkat untuk menutup panel saat Anda mengeklik di luarnya.
- **Pintasan ala Mac** hanya memakai `RegisterHotKey` dan pesan jendela standar (WM_CLOSE, minimalkan); tidak ada yang disuntikkan ke app lain. Bisa dimatikan di Pengaturan → Umum.
- **Perintah sistem** (kunci layar, kalkulator, dsb.) dijalankan dari jalur absolut `System32`, bukan lewat PATH, sehingga tidak bisa dibajak oleh berkas dengan nama sama.
- **Memindahkan jendela.** Stage Manager memindahkan jendela app lain ke luar layar dan mengembalikannya. Tidak ada isi jendela yang
  disimpan di disk selain daftar posisi sementara (`parked.txt`) agar jendela bisa dipulihkan.
- **Mulai otomatis** memakai nilai `ABRA` di `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (tanpa hak Administrator); bisa
  dimatikan dari Pengaturan ABRA atau Pengaturan Windows → Aplikasi → Startup.
- Perangkat lunak ini disediakan **apa adanya** tanpa jaminan (lihat [LICENSE](../LICENSE.md)). Kode sumbernya terbuka untuk
  diperiksa.

## 14. Pemecahan masalah

| Masalah | Solusi |
|---|---|
| Taskbar hilang setelah ABRA mati | Jalankan `abra.exe` lalu keluar normal, atau biarkan `abra-guard.exe` memulihkannya. Pastikan kedua berkas satu folder. |
| Ada jendela yang "hilang" | Kemungkinan diparkir Stage Manager. Jalankan ABRA lagi (ia memulihkan jendela terparkir saat mulai), atau matikan Stage Manager. |
| Pintasan tidak bekerja | Program lain mungkin memakai kombinasi yang sama; cek `abra.log`. |
| Menu app tidak muncul di menu bar | App itu tidak memakai menu Win32 klasik, jadi menu standar dipakai. |
| Menu bar/dock tidak terlihat di layar penuh | Aktifkan Pengaturan → Umum → **Selalu di atas jendela layar penuh** (bawaan aktif). Game dengan layar penuh eksklusif tidak bisa ditimpa. |
| Ikon desktop tidak terlihat | Pengaturan → Umum → matikan **Sembunyikan ikon desktop saat ABRA berjalan**. Ikon kembali saat ABRA ditutup. |
| Ikon dock terlihat berbeda dari aslinya | Matikan **Ikon seragam ala macOS** di tab DOCK dan setel **Gaya ikon** ke Bawaan. |
| Kursor tetap bulat setelah ABRA mati | Jalankan ABRA lagi dan keluar normal, atau biarkan `abra-guard.exe` memulihkannya; bisa juga dengan Pengaturan Windows → Bluetooth & perangkat → Mouse → Penunjuk mouse. |
| Melaporkan bug | Sertakan versi Windows, langkah mengulang, dan isi `abra.log` pada Issues proyek. |

## 15. Batasan yang diketahui

- Menyeret kartu Stage Manager ke panggung belum tersedia (gunakan Shift-klik). Kartu hidup memakai Windows.Graphics.Capture, jadi
  memerlukan Windows 10 versi 1903 atau lebih baru (Windows 11 sudah memenuhi).
- App yang dijalankan sebagai Administrator tidak bisa dikelola Stage Manager.
- Fokus, Hotspot, dan Mode Pesawat di Pusat Kontrol hanya membuka halaman pengaturan Windows yang sesuai.
- Isi menu klasik yang diisi "malas" oleh app (misalnya daftar berkas terbaru) tidak tampil di menu bar.
- "Paksa Keluar" membuka Task Manager.
- Diuji pada Windows 11 dengan satu layar utama.

## 16. Mencopot pemasangan

- **Installer:** Pengaturan Windows → Aplikasi → ABRA → Copot. Installer menghentikan ABRA, memulihkan taskbar, dan menghapus entri
  mulai otomatis.
- **Zip portabel:** matikan **Jalankan saat login Windows** di Pengaturan ABRA, keluar dari ABRA (taskbar kembali), lalu hapus
  foldernya.
- Opsional: hapus `%APPDATA%\ABRA` untuk membuang semua pengaturan.
