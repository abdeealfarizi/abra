# Panduan Pengguna ABRA

ABRA mengganti taskbar Windows 11 dengan pengalaman desktop modern: **Deck**, **Visor (Capsule)**, **Rail**, **Quick Search**,
**Apps**, **Overview**, **Scene**, **Quick Panel**, dan **Screen Corners**, semuanya bergaya Crystal
dan ringan.

> [!WARNING]
> **Akses awal (Early Access).** ABRA masih aktif dikembangkan. Bug bisa muncul, tampilan bisa berubah, dan beberapa
> pengaturan bisa berpindah atau kembali ke bawaan di versi berikutnya. Laporkan masalah atau ide di
> [Issues](https://github.com/abdeealfarizi/abra/issues). Sejak 0.5, ABRA bisa memperbarui dirinya sendiri
> (Pengaturan → Umum → Tentang).

> Panduan ini berbahasa Indonesia. Ringkasan dalam bahasa Inggris, cara build, dan informasi pengembangan ada di
> [README](../README.md).

## Daftar isi

1. [Memasang dan memulai](#1-memasang-dan-memulai)
2. [Deck](#2-deck)
3. [Visor](#3-visor)
4. [Rail](#4-rail)
5. [Quick Panel](#5-quick-panel)
6. [Quick Search](#6-quick-search)
7. [Apps](#7-apps)
8. [Overview](#8-overview)
9. [Scene](#9-scene)
10. [Screen Corners](#10-screen-corners)
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

**"Antivirus bilang ABRA virus/trojan."** Itu *false positive*. ABRA belum bertanda tangan digital, masih baru, dan melakukan hal
yang memang wajib dilakukan sebuah shell desktop tapi terlihat mencurigakan bagi pemindai heuristik: menyembunyikan taskbar,
hook keyboard untuk Alt+Tab, memindahkan jendela app lain, dan berjalan saat login. ABRA tidak punya telemetri dan seluruh kode
sumbernya terbuka. Cara memastikan berkas Anda asli:

1. Unduh hanya dari halaman **Releases** proyek ini.
2. Di PowerShell jalankan `Get-FileHash .\ABRA-Setup-<versi>.exe -Algorithm SHA256`, lalu bandingkan hasilnya dengan nilai
   **sha256** di samping berkas pada halaman Releases. Harus sama persis.
3. Atau build sendiri dari kode sumber.

Bila Windows Defender mengarantina berkasnya, pulihkan dari *Keamanan Windows → Perlindungan virus & ancaman → Riwayat
perlindungan*, tapi hanya setelah sidik jarinya cocok.

Begitu `abra.exe` dijalankan, taskbar bawaan Windows disembunyikan lalu Rail, Visor, dan Deck muncul. Deck bersembunyi sampai kursor dibawa ke tepi bawah layar.

- **Keluar:** klik kanan ikon di Deck atau ikon ABRA di area tray → **Keluar dari ABRA**. Taskbar dipulihkan otomatis.
- **Ikon tray:** klik kiri membuka Pengaturan, klik kanan membuka menu (Pengaturan, Sembunyikan taskbar Windows, Keluar). Di
  Windows 11 ikon baru biasanya masuk ke panah `^` dulu; seret keluar bila ingin selalu terlihat.
- **Bila ABRA mati mendadak:** `abra-guard.exe` otomatis memulihkan taskbar (dan jendela yang sedang diparkir Scene). Bila
  tidak, jalankan `abra.exe` lagi lalu keluar secara normal.
- **Buka pengaturan:** klik kanan Deck → **Pengaturan ABRA…**, ikon roda gigi di Deck, atau logo bulat di Rail → **Pengaturan ABRA…**.
- **Mulai otomatis saat login:** Pengaturan → Umum → **Jalankan saat login Windows**.
- **Pembaruan:** ABRA memeriksa versi baru di GitHub kira-kira tiap 12 jam. Bila ada, Visor menampilkan pill "Pembaruan
  tersedia" dan Pengaturan → Umum → Tentang menampilkan tombol **Perbarui ke …**. Satu klik mengunduh installer, mencocokkan
  sidik jari SHA-256-nya dengan yang tercatat di GitHub (installer yang tidak cocok ditolak), memasangnya, lalu membuka ABRA lagi.
  Versi portabel (zip) hanya membuka halaman unduhan. Pemeriksaan otomatis bisa dimatikan di tempat yang sama.

Hanya satu ABRA yang berjalan; menjalankannya lagi membuka Pengaturan ABRA yang sudah berjalan. Taskbar Windows tidak dimatikan selamanya: ia
hanya disembunyikan selama ABRA berjalan (bisa dimatikan di Pengaturan → Umum → **Sembunyikan taskbar Windows**).

## 2. Deck

- **Buka/aktifkan app:** klik ikon. Klik lagi pada app yang sedang di depan untuk meminimalkannya.
- **Tambah app:** seret berkas atau pintasan (`.exe`, `.lnk`) ke Deck; atau klik kanan Deck → **Tambah app ke Deck…**; atau klik kanan
  app yang sedang berjalan → **Simpan di Deck**. App Microsoft Store (termasuk WhatsApp) didukung.
- **Susun ulang:** seret ikon yang disematkan ke posisi lain.
- **Hapus:** klik kanan ikon → **Hapus dari Deck**, atau seret ikon keluar dari Deck.
- **Ganti ikon:** klik kanan → **Ubah ikon…** (png, jpg, ico, bmp, gif). **Kembalikan ikon asli** untuk membatalkan. Berlaku juga untuk
  **tombol Mulai** di ujung kiri Deck.
- **Tombol App Grid:** tepat setelah tombol Mulai ada ikon kisi titik berwarna. Klik untuk membuka Apps; klik kanan → **Ubah ikon…** untuk menggantinya (atau `--set-icon launchpad <gambar>`).
- **Ikon seragam:** semua ikon diberi badan squircle yang sama besar dengan bayangan lembut; ikon berbentuk bebas
  diletakkan di atas pelat putih. Matikan di Pengaturan → DECK bila ingin ikon asli Windows.
- **Pratinjau jendela:** diamkan kursor di atas ikon app yang berjalan. Klik pratinjau untuk membuka jendelanya.
- **Zoom:** ikon membesar saat kursor lewat; ukuran, jarak, zoom, dan jarak dari tepi diatur di tab **Ukuran**.
- **Paket ikon (opsional):** letakkan folder berisi PNG ikon (mis. koleksi ikon pilihanmu) di `%APPDATA%\ABRA\iconpack`. ABRA mencocokkan app Windows yang punya padanan jelas dengan nama berkas di folder itu: Explorer → `Finder`, Kalkulator → `Calculator`, Notepad → `TextEdit`, Task Manager → `Activity Monitor`, Terminal/CMD/PowerShell → `Terminal`, Pengaturan → `System Settings`, Microsoft Store → `App Store`, Mail, Calendar, Maps, Photos, Weather, dan sebagainya, serta tombol Apps → `Apps`. Browser dan app pihak ketiga tidak pernah diganti. Ikon dari paket tampil apa adanya (tanpa pelat dan gaya tambahan) di Deck, Quick Search, dan Apps. Ikon yang Anda pilih manual lewat "Ubah ikon…" selalu menang. Matikan di Pengaturan → Tampilan → "Paket ikon". ABRA **tidak membawa** ikon apa pun; Anda memasang paketnya sendiri dan bertanggung jawab atas lisensinya.
- **Tumpukan Unduhan** (sebelum tempat sampah): klik untuk melihat 10 berkas terbaru di folder Unduhan dan membukanya, atau pilih "Buka folder Unduhan". Berkas yang masih diunduh disembunyikan. Bisa dimatikan di Pengaturan → Deck.
- **App terakhir dipakai:** tiga app yang baru saja Anda tutup (bukan sematan) tetap muncul di akhir daftar app agar mudah dibuka lagi; daftarnya disimpan di `recents.txt`. Bisa dimatikan di Pengaturan → Deck.
- **Tooltip** nama ikon berekor kecil yang menunjuk ke ikon.
- **Tempat sampah** ada di ujung kanan Deck dengan ikon kaca: kosong, atau berisi gumpalan kertas bila Recycle Bin ada isinya (diperbarui tiap beberapa detik). Klik kanan untuk membuka atau mengosongkannya (Windows tetap meminta konfirmasi). Bila pack ikonmu punya ikon tempat sampah, ikon pack yang dipakai.
- **Mode Deck** (tetap, menimpa, sembunyi otomatis, sentuh tepi) ada di halaman **Deck**. Material permukaan (Blur, Crystal, atau Hitam buram) dan tema terang/gelap diatur sekali untuk semua permukaan di halaman **Tampilan**.
- **Efek minimize** (Pengaturan → Deck): **Funnel** (bawaan; jendela mengalir seperti corong ke ikonnya di Deck dan keluar lagi saat ikon diklik) atau **Skala** (mengecil lurus ke ikon, lebih singkat). Animasi minimize, buka, dan tutup jendela dapat dimatikan di Pengaturan → Umum; bayangan jendela aktif di Pengaturan → Jendela.

**Gaya ikon:** Pengaturan → Tampilan → *Gaya ikon* mengubah semua ikon Deck, tombol Apps, dan grid Apps: **Gelap** menggelapkan latar ikon, **Bening** membuatnya abu-abu kaca tembus pandang, **Berwarna** memetakan warna ikon ke satu warna tint (Biru, Ungu, Merah Muda, Oranye, Hijau). Ikon yang Anda ganti sendiri tidak ikut berubah.

## 3. Visor

Pil kecil di tengah atas layar dengan bayangan tipis di bawahnya. Materialnya (**Blur**, **Crystal**, atau **Hitam**)
dipilih lewat *Material Visor* di Pengaturan → Tampilan; teksnya selalu putih.

**Saat diam** Visor menampilkan **sapaan & info** yang berganti pelan tiap ±9 detik: sapaan ("Selamat malam, Rani",
hanya tulisan di tengah), jam, tanggal, dan baterai (ikon di kiri, nilai di kanan). Di Pengaturan → Visor → *Saat Visor
diam* → **Tampilkan** ada tiga pilihan: **Sapaan & info**, **Musik & lirik**, dan **Kosong**; di sana juga **Nama
panggilan** dan **Teks sapaan**.

**Musik & lirik:** selama ada lagu (diputar maupun dijeda), Visor diam melebar dan sedikit turun: sampul kecil dengan
ikon app pemutarnya (Spotify, browser, Media Player…) di sudutnya, tombol **mundur · putar/jeda · maju** di bawah sampul,
dan lirik di kanan dengan gaya yang sama seperti tab Putar (baris aktif tebal, baris lain redup dan buram, bergulir naik).
Tombolnya bisa langsung diklik; diam di atas tombol tidak membuka panel. Diam atau klik di bagian lain membuka panel
seperti biasa. Lagu tanpa lirik tersinkron menampilkan judul dan artis.

**Aktivitas** yang sedang berlangsung menggantikan sapaan, dari yang paling penting: berkas sedang diseret ke Visor → timer
yang tinggal di bawah 1 menit → unduhan → lirik → lagu (sampul + visualizer berwarna sampul) → baterai baru mulai diisi → timer
berjalan → jumlah berkas di Rak. Titik **oranye** (mikrofon) atau **hijau** (kamera) kecil muncul di tepi kanan bila ada
app yang sedang memakainya.

**Membuka panel:** diamkan kursor di Visor ±0,3 detik (atau klik, bila *Buka otomatis saat kursor di Visor* dimatikan).
Panel menutup sendiri sesaat setelah kursor keluar. Panel punya tiga tab (klik, atau gulir mouse untuk berpindah):

- **Putar**: kartu lagu (sampul, judul, artis, penggeser posisi yang bisa diseret, mundur · putar/jeda · maju) dan
  **lirik besar**: baris yang sedang dinyanyikan tebal dan terang, baris sebelum/sesudahnya redup dan sedikit buram, dan
  isinya bergulir naik saat baris berganti.
- **Rak**: berkas yang Anda taruh di Visor. Klik untuk membuka; klik kanan untuk *Tampilkan di folder*, *Salin jalur*,
  atau *Hapus dari Rak*; **Kosongkan** menghapus semuanya dari Rak. Rak hanya menyimpan **jalur** berkas (isinya tidak
  disalin) dan kosong lagi saat ABRA ditutup.
- **Timer**: dua kotak. Kiri: timer (−5 menit · mulai/jeda · ulang · +5 menit). Kanan: **kalender** horizontal: bulan,
  lima tanggal di sekitar tanggal terpilih (hari ini berwarna aksen), dan acara tanggal itu dari Kalender Windows
  ("Tidak ada acara hari ini" bila kosong). Klik tanggal untuk melihat acaranya; kalender kembali ke hari ini saat panel
  ditutup.

Di kanan atas panel ada baterai dan tombol Pengaturan ABRA.

**Menyeret berkas ke Visor:** saat berkas dari Explorer diseret ke Visor, muncul empat zona: **Simpan ke Rak**,
**Kirim** (ke HP, lihat di bawah), **Salin ke…** (pilih folder, lalu disalin dengan jendela salin Windows), dan **Buka
dengan…** (dialog "Buka dengan" Windows untuk berkas pertama).

### Kirim ke HP

Taruh berkas di zona **Kirim** (atau klik kanan berkas di Rak → **Kirim ke HP**). Tab Rak menampilkan **kode QR**.
Arahkan kamera HP (Android atau iPhone, tanpa aplikasi tambahan) ke QR itu, buka tautannya, lalu ketuk **Unduh** di
halaman yang muncul. Kemajuan tampil di kartu dan di Visor tertutup (busur hijau dengan panah ke atas), lalu
"Terkirim ✓".

- Laptop dan HP harus tersambung ke **Wi-Fi/jaringan yang sama**.
- Saat pertama dipakai, Windows Firewall bisa menanyakan izin untuk ABRA: pilih **Jaringan pribadi** agar HP bisa
  mengunduh.
- Sesi berakhir saat **Batal** (atau **Tutup** setelah selesai) diklik, setelah 15 menit tanpa aktivitas, atau saat ABRA ditutup.
- Matikan zona ini lewat Pengaturan → Visor → Aktivitas → "Kirim ke HP lewat QR (Wi-Fi yang sama)".

Perilaku (tetap, menimpa, sembunyi otomatis, saat tertimpa) dan kebulatan sudut diatur di Pengaturan → Visor; Visor bisa
dimatikan dengan **Aktifkan Visor**.

### Unduhan di Visor

Saat ada unduhan berjalan, Visor tertutup sedikit memanjang: **ikon app pengunduh** di kiri (Microsoft Store, Steam, atau
browser yang mengunduh, mis. Brave), dan di kanan **busur hijau** yang terisi searah jarum jam dari ±jam 8 sampai ±jam 4
dengan panah unduh di tengahnya. Angka di bawahnya adalah **persen** (tanpa tanda %).

- **Microsoft Store** dan **Steam** memberi persen yang sebenarnya.
- **Browser** tidak memberi tahu ukuran total, jadi busurnya **berputar** dan angkanya menunjukkan ukuran yang sudah
  terunduh (mis. `26M`). ABRA mengenali berkas sementara `.crdownload` / `.part` di folder Unduhan.
- Saat selesai, busur penuh dengan tanda centang tampil ±3 detik.

Matikan lewat Pengaturan → Visor → Aktivitas → "Tampilkan unduhan (Store, Steam, peramban)".

### Lirik di Visor

Saat lagu diputar dan liriknya tersedia, Visor tertutup menampilkan **baris lirik yang sedang dinyanyikan** di tengah, di
antara sampul (kiri) dan visualizer (kanan); barisnya berganti mengikuti posisi lagu. Tab **Putar** di Visor terbuka
menampilkan lirik besar seperti di atas.

Sumber lirik, berurutan:
1. **Berkas `.lrc` Anda sendiri** di `%APPDATA%\ABRA\lyrics`, bernama `Artis - Judul.lrc` atau `Judul.lrc` (huruf besar/kecil
   tidak berpengaruh). Buka foldernya lewat Pengaturan → Visor → Lirik → "Folder lirik (.lrc)".
2. **Cache** hasil pencarian sebelumnya.
3. **LRCLIB online** (bila diizinkan, aktif bawaan). Lagu yang tidak ditemukan dicoba lagi setelah 3 hari.

Lirik tanpa cap waktu hanya tampil di kartu Lirik. Baris lirik di Visor tertutup bisa dimatikan lewat "Tampilkan lirik di Visor".

### Tab Tugas dan pengingat

Halaman **Tugas** berisi daftar tugas Anda dengan angka besar jumlah yang sudah selesai. Klik sebuah tugas untuk mencentangnya;
**Hapus yang selesai** membersihkan yang sudah dicentang. Tombol **Tambah tugas** (atau `Ctrl+Alt+N` dari mana saja) berubah
menjadi kolom ketik langsung di Visor: ketik lalu tekan Enter (kolomnya tetap terbuka untuk tugas berikutnya), Esc untuk menutup.
Saat Anda mengetik, Visor langsung menampilkan jam pengingat yang dikenali. Bila Visor dimatikan, `Ctrl+Alt+N` membuka Quick Search
dalam mode **Tugas** sebagai gantinya.

Pindah halaman Visor (Putar, Rak, Timer, Tugas) lewat titik di bagian bawah atau roda mouse; tiap halaman punya warnanya sendiri.

Waktu di awal teks menjadikannya pengingat:
- `15:30 rapat tim` – hari ini pukul 15.30 (besok bila jamnya sudah lewat), `09.15` juga bisa;
- `10m minum` / `2j istirahat` – sepuluh menit / dua jam dari sekarang;
- `besok 08:00 kirim laporan` – besok pagi; awalan `ingatkan` boleh dipakai.

Saat jatuh tempo, Visor menampilkan pill pengingat yang menetap sampai diklik (klik membuka tab Tugas). Bila Visor mati atau Anda
sedang layar penuh, pengingat muncul sebagai balon di area tray. Tugas disimpan di `%APPDATA%\ABRA\todo.txt`.

### Pill dan konteks

Pill adalah kabar singkat di Visor: pengingat, pembaruan, dan perangkat yang tersambung atau terputus (earbuds, mouse, keyboard
Bluetooth, flashdisk). Klik pill untuk menindaklanjutinya (mis. membuka flashdisk), klik kanan untuk menutupnya. Pilih yang ingin
tampil di Pengaturan → Visor → **Pill & Konteks**. Di sana juga ada **Aurora warna album**: saat musik main, gelombang cahaya
lembut berwarna sampul album bergerak di bagian bawah Visor dan Capsule, naik-turun mengikuti bass, vokal, dan treble.

### Ukuran dan jarak

Pengaturan → Visor → **Ukuran** (80–130%) mengubah besar Visor dan Capsule. Untuk bentuk Capsule ada **Jarak dari tepi atas**.
Berganti Visor ↔ Capsule memakai animasi "tetesan air".

## 4. Rail

Rail aktif secara bawaan. Matikan atau nyalakan di Pengaturan → Umum → **Aktifkan Rail**.

**Sisi kiri**
- **Logo bulat ABRA:** Tentang PC Ini, Pengaturan Sistem, Microsoft Store, Pengaturan ABRA, Aplikasi, Overview, Paksa Keluar
  (membuka Task Manager), Tidur, Mulai Ulang, Matikan, Kunci Layar. Aksi daya meminta konfirmasi lebih dulu.
- **Nama app aktif** (tebal): Tentang, Sembunyikan, Sembunyikan Lainnya, Tampilkan Semua, Keluar.
- **Menu app:** bila app aktif memiliki menu Win32 klasik (File, Edit, dst.), menu aslinya ditampilkan dan bisa diklik. App tanpa menu
  klasik mendapat menu yang disesuaikan dengan app-nya dan mengirim pintasan keyboard resmi app itu:
  - **Browser** (Chrome, Edge, Brave, Firefox, Opera, Vivaldi): tab baru/tutup/buka lagi, jendela penyamaran, riwayat, unduhan,
    bookmark, pindah dan geser tab, alat pengembang.
  - **File Explorer**: jendela/tab baru, folder baru, properti, salin jalur, ganti nama, mode tampilan ikon/daftar/detail, panel
    pratinjau, menu Buka (mundur, maju, folder induk, bilah alamat).
  - **Word, Excel, PowerPoint**: simpan sebagai, ganti, format tebal/miring/garis bawah dan perataan.
  - **VS Code dan editor sejenis**: palet perintah, buka cepat, terminal, cari di berkas, ke baris, jalankan/debug.
  - **Spotify**: putar/jeda, lagu berikut/sebelumnya, volume.
  - App lain memakai menu umum (Berkas, Edit, Tampilan, Jendela, Bantuan).

  **Tentang [app]** membuka jendela Properti berkas app tersebut (untuk app Store: halaman Aplikasi di Pengaturan Windows), dan
  **Bantuan → Panduan ABRA** membuka panduan ini. Setelah satu menu terbuka, gerakkan kursor ke judul lain untuk berpindah tanpa klik.

**Perilaku Rail** (Pengaturan → Rail): **Tetap** (jendela yang dimaksimalkan berada di bawah Rail), **Menimpa** (Rail selalu tampil
tanpa memesan ruang), atau **Pintar** (Rail menyingkir saat tertutup jendela yang dimaksimalkan dan muncul lagi saat kursor menyentuh
tepi atas layar).

**Sisi kanan** (dari kanan ke kiri): jam (klik membuka Pusat Notifikasi Windows), **Quick Panel**, **Quick Search**, Wi-Fi, baterai
(klik membuka pengaturan Windows terkait). Ada ikon Scene saat fiturnya aktif.

**Menu Wi-Fi:** klik ikon Wi-Fi untuk membuka menu: sakelar Wi-Fi, **Jaringan Dikenal** (lingkaran biru = tersambung,
ikon sinyal, gembok untuk jaringan bersandi), **Jaringan Lain** yang bisa dibuka, dan **Pengaturan Wi-Fi…**. Klik jaringan
dikenal atau jaringan terbuka untuk langsung menyambung; jaringan bersandi yang baru membuka daftar jaringan Windows agar
Anda bisa memasukkan sandinya.

> Sejak Windows 11 24H2, Windows hanya memberikan daftar jaringan sekitar kepada aplikasi desktop bila **Lokasi** aktif
> (Pengaturan Windows → Privasi & keamanan → Lokasi, termasuk "Izinkan aplikasi desktop mengakses lokasi"). Bila Lokasi mati,
> menu tetap berfungsi dalam mode terbatas: status tersambung, tombol **Nyalakan Lokasi…**, dan **Jaringan Lain…** (daftar
> jaringan Windows). ABRA tidak membaca atau menyimpan lokasi Anda.

**Latar Rail** (Pengaturan → Rail): **Transparan** (tanpa latar), **Terang** (putih rata), **Gelap** (hitam rata), atau **Material** (bawaan; strip yang memakai material global, mis. Blur: kabut tebal; teksnya mengikuti tema).
Warna teks menyesuaikan wallpaper atau warna latar. Tema terang/gelap tidak mengubah Rail.

**Nama app di kiri** mengikuti jendela yang terakhir Anda klik dengan mouse (juga saat jendela itu tidak berpindah fokus), dan jendela
depan bila fokus berpindah lewat keyboard atau Scene.

Secara bawaan Deck, Rail, dan Visor **menyingkir saat ada video, game, atau app lain yang layar penuh** (jendela yang hanya
dimaksimalkan tidak dihitung). Bila Anda ingin mereka tetap di atas, nyalakan Pengaturan → Umum → **Selalu di atas jendela layar penuh**.

## 5. Quick Panel

Klik ikon dua sakelar di Rail. Panel berisi modul kaca mengambang: Wi-Fi (dengan nama jaringan), Bluetooth, Berbagi Terdekat (padanan AirDrop), Playing, Scene, Overview, Mode Gelap, Tangkapan Layar, Fokus, serta slider **Suara** dan **Layar** (kecerahan). Klik modul untuk menyalakan/mematikan; seret slider untuk mengubah nilai. Pada modul **Wi-Fi** dan **Bluetooth**, klik
lingkaran ikonnya untuk menyalakan/mematikan, dan klik bagian lain modul (atau klik kanan modul bulat kecil) untuk membuka
**pemilih jaringan / perangkat**: Quick Panel menutup, lalu jendela Wi-Fi/Bluetooth muncul di pojok kanan atas dengan
animasi yang sama seperti Quick Panel (sudutnya sebesar kartu slider Suara/Layar). Menu Bluetooth menampilkan perangkat yang sudah
dipasangkan (lingkaran biru = tersambung). Klik **headphone atau speaker** untuk langsung menyambung/memutusnya (label
"Menyambung…" / "Memutus…" tampil sampai selesai, menu tetap terbuka). Perangkat lain (keyboard, mouse, ponsel) membuka
Pengaturan Bluetooth Windows, karena Windows hanya menyediakan cara sambung langsung untuk perangkat audio. Kenop slider berupa pil putih; selama diseret ia berubah jadi kaca bening dan sedikit membesar ("tetes air"), sama seperti slider di Visor. Tombol bulat di kanan modul **Suara** membuka pemilih keluaran suara Windows (speaker/headphone, sama dengan Win+Ctrl+V).

Bila ada app yang sedang memakai **mikrofon** (titik oranye) atau **kamera** (titik hijau), namanya tampil sebagai chip kecil di atas modul. Klik chip untuk membuka pengaturan privasi mikrofon/kamera Windows. ABRA hanya membaca catatan pemakaian milik Windows; ia tidak mengakses mikrofon atau kamera.

**Edit Kontrol…** membuka mode edit dan galeri **Cari Kontrol**:
- badge **−** menghapus modul; seret modul ke modul lain untuk menukar urutan; **Selesai** menyimpan.
- Galeri dikelompokkan per kategori (Konektivitas, Layar & Kecerahan, Suara, dan seterusnya) dengan kolom cari dan sidebar berikon warna. Klik kontrol untuk menambahkannya.
- **Klik kanan** modul (di mode edit) untuk mengubah ukuran: **Kecil** (bulat), **Sedang** (kapsul), atau **Besar** (2×2). Slider dan Playing berukuran tetap.
- Fokus, Hotspot, Berbagi Terdekat, dan Mode Pesawat membuka halaman pengaturan Windows yang sesuai.

**HUD volume dan kecerahan:** saat Anda menekan tombol volume/kecerahan, kapsul kaca muncul di kanan atas (di bawah Rail). Judulnya nama perangkat keluaran yang sedang dipakai (mis. "Headphones") atau "Layar" untuk kecerahan, di bawahnya slider tipis dengan ikon di kedua ujung dan 16 titik skala. Bisa diseret dan hilang sendiri.

## 6. Quick Search

`Ctrl+Space` atau ikon kaca pembesar di Rail. Quick Search muncul memudar sebagai kapsul "Quick Search Search". Arahkan kursor ke atasnya: kapsul memendek dan empat tombol bulat muncul di kanan (Apps, Berkas, Aksi, Papan Klip); menunjuk sebuah tombol menampilkan nama mode dan pintasannya di kapsul. Memilih mode membuat kapsul melebar menelan tombol, lalu panel tumbuh ke bawah dan isinya memudar masuk. Begitu Anda mengetik, kapsul berubah menjadi panel hasil.

**Mencari:** ketik nama aplikasi (fuzzy, inisial didukung), ekspresi hitung seperti `12*(3+4)`, nama berkas, atau nama aksi. Hasil dikelompokkan: kalkulator, aplikasi, aksi, berkas, lalu baris **Cari di web**. Chip **Aplikasi / Berkas Saya / Aksi** di bawah kolom input menyaring jenis hasil. Panah atas/bawah memilih, `Enter` membuka, `Esc` membersihkan input lalu menutup. Baris web membuka Google di browser Anda **hanya bila dipilih**; ABRA sendiri tidak mengirim apa pun lewat jaringan.

**Empat mode** (tombol bulat, atau `Ctrl+1` sampai `Ctrl+4`, yang berlaku di mana saja di Quick Search):

| Pintasan | Mode | Isi |
|---|---|---|
| `Ctrl+1` | **Apps** | Seksi **Saran** (app di Deck) lalu per kategori, masing-masing satu baris dengan **Lihat Semua**; ketik untuk mencari (lihat bagian 7) |
| `Ctrl+2` | **Berkas Saya** | Tanpa kata kunci: **Terbaru** (berkas yang baru Anda buka di Windows, dengan thumbnail). Chip **Gambar, Video, Dokumen, Musik** menyaring jenisnya. Dengan kata kunci: mencari nama berkas dan folder di Desktop, Dokumen, Unduhan, Gambar, Video, Musik, dan OneDrive (sampai 5 tingkat, hasil teratas 20). Bukan pencarian seluruh disk |
| `Ctrl+3` | **Aksi** | Perintah cepat: Buat Catatan, Mulai Timer, Kirim Email, Ambil Tangkapan Layar, Kunci Layar, Tampilkan Desktop, Ganti Mode Gelap, Scene, Overview, Kalkulator, Task Manager, Pengaturan, Hapus Riwayat Papan Klip |
| `Ctrl+4` | **Papan Klip** | Teks yang Anda salin dalam 8 jam terakhir ("teks · Disalin 15.20"). `Enter` atau tombol salin bulat menyalinnya kembali; tombol **⋯** → Hapus Riwayat |

**Quick keys untuk Aksi:** tiap aksi bisa dijalankan dengan huruf singkat. Di mode Aksi, ketik `nn` lalu `Enter` untuk Buat Catatan. Pilih sebuah aksi lalu tekan `Tab` untuk mengisi atau mengganti quick key-nya (sampai 6 huruf/angka), `Enter` menyimpan, `Esc` membatalkan. Tersimpan di `actions.txt` pada folder konfigurasi.

**Riwayat Papan Klip** bisa dimatikan di Pengaturan → Umum → **Riwayat Papan Klip di Quick Search**. Riwayat hanya disimpan di memori (hilang saat ABRA ditutup), tidak pernah ditulis ke disk atau dikirim ke mana pun. Isi yang ditandai pengelola kata sandi diabaikan. Hanya teks.

## 7. Apps

Tombol **Apps** di Deck (ikon kisi titik berwarna), `Ctrl+Shift+Space`, `Ctrl+1` di Quick Search, menu logo → Apps, atau screen corner. Panel kaca di tengah atas dengan judul "Applications", chip kategori (Semua, Produktivitas, Kreativitas, Hiburan, Utilitas, Sistem), dan grid 5 kolom berikon seragam.

- **Cari:** langsung ketik; grid ikut menyaring.
- **Pindah:** panah (atas/bawah/kiri/kanan) atau arahkan mouse; gulir mouse menggeser baris. `Enter` atau klik membuka.
- **Tutup:** `Esc` (pertama membersihkan pencarian, lalu kembali ke Quick Search, lalu menutup), atau klik di luar panel.

## 8. Overview

`Ctrl+↑`, menu logo, Quick Panel, atau screen corner. Semua jendela terbuka ditata dalam grid thumbnail hidup; judul muncul saat kursor
di atas jendela. Bar di atas menampilkan Virtual Desktop (klik untuk berpindah, **+** untuk membuat desktop baru). Klik jendela untuk
mengaktifkannya; `Esc` atau klik area kosong untuk keluar. Jendela yang diminimalkan tidak ikut ditampilkan.

## 9. Scene

Nyalakan di Pengaturan → **Stage** → **Aktifkan Scene**, lewat Quick Panel, atau lewat screen corner.

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

**Pengaturan Scene** (tab **Stage**):
- **Posisi daftar jendela:** kiri atau kanan layar.
- **Ukuran kartu**, **jarak antar kartu**, **jumlah kartu** (grup yang melebihi batas dikembalikan ke layar), **kecepatan animasi** (50–200%).
- **Judul jendela saat hover:** nama app muncul di atas kartu yang disorot.
- **Klik wallpaper menyimpan semua jendela:** dapat dimatikan.
- **Jeda media app di latar:** media (browser, pemutar musik) milik app yang keluar panggung dijeda lalu dilanjutkan saat app kembali.
  Hanya untuk app yang mendaftarkan sesi media Windows.
- **Ikon desktop:** sembunyi, otomatis (tampil saat panggung kosong), atau tampil.
- **Program dikecualikan:** tulis potongan jalur atau nama exe (satu per baris, huruf kecil) di `stage_exclude.txt` pada folder
  konfigurasi ABRA (lihat [Pengaturan](#11-pengaturan)). Jendela program itu tidak pernah diparkir.

App yang berjalan sebagai Administrator tidak bisa dipindah oleh proses biasa, jadi Scene membiarkannya apa adanya.

## 10. Screen Corners

Pengaturan → Jendela → **Screen Corners**. Klik tombol tiap sudut untuk berputar antar aksi: Tidak ada, Overview, Apps,
Tampilkan Desktop, Scene, Pusat Notifikasi, Kunci Layar. Aksi berjalan setelah kursor diam di sudut sekitar 0,25 detik dan
diabaikan saat tombol mouse ditekan, jadi aman saat menyeret jendela.

## 11. Pengaturan

Jendela Pengaturan ABRA bergaya panel pengaturan modern: sidebar berikon warna di kiri dan halaman berisi kartu grup di kanan (gulir dengan roda mouse bila isinya panjang). Tutup jendela dengan tombol **✕** di kanan atas, lampu merah di kiri atas, atau `Esc`; judul bisa diseret. Di sidebar ada kolom **Cari** (ketik untuk menyaring halaman; `Enter` membuka yang pertama, `Esc` menghapus) dan kartu akun Windows; tombol **◀ ▶** di bilah judul kembali/maju antar halaman yang sudah dibuka.

| Halaman | Isi |
|---|---|
| **Umum** | Bahasa (Indonesia/English/Español), jalankan saat login, sembunyikan taskbar Windows, sembunyikan ikon desktop, selalu di atas jendela layar penuh, animasi jendela, bayangan jendela aktif |
| **Deck** | Aktifkan Deck, perilaku, ikon seragam, ukuran ikon, jarak, zoom, jarak dari tepi, kebulatan sudut |
| **Visor** | Aktifkan Visor, perilaku, kebulatan sudut, buka otomatis saat kursor di Visor |
| **Rail** | Aktifkan Rail, latar (Transparan, Terang, Gelap, Material), jam 24 jam |
| **Tampilan** | Kartu pratinjau **Otomatis · Terang · Gelap**; **Tema**: warna aksen (Multiwarna = ikut warna aksen Windows, lalu 8 warna), kartu **gaya ikon & widget** Bawaan · Gelap · Bening · Berwarna dengan pilihan Terang · Gelap · Otomatis, warna tint, paket ikon; **Kaca**: material global (Blur, Crystal, Hitam buram), material Visor, tingkat blur, transparansi, refraksi tepi kaca |
| **Scene** | Semua pengaturan Scene |
| **Screen Corners** | Aksi untuk empat screen corner |
| **Quick Search** | Riwayat Papan Klip |

Semua perubahan langsung berlaku. Berkas konfigurasi ada di folder `%APPDATA%\ABRA`:

| Berkas | Isi |
|---|---|
| `settings.cfg` | Pengaturan (teks `kunci=nilai`) |
| `pins.txt` | App yang disematkan di Deck |
| `icons.txt` dan folder `icons\` | Ikon kustom |
| `cc_layout.txt` | Susunan modul Quick Panel |
| `stage_exclude.txt` | Program yang dikecualikan dari Scene |
| `abra.log` | Log (berguna untuk laporan bug) |

**Opsi baris perintah** yang berguna untuk pengguna: `--keep-taskbar` (jangan sembunyikan taskbar Windows), `--open-settings`,
`--add-app <jalur>`, dan `--set-icon <app|start> <gambar>`. Variabel lingkungan `NACRE_CONFIG_DIR` memakai folder konfigurasi lain
(sekaligus menjadi instance terpisah).

## 12. Pintasan keyboard

| Pintasan | Aksi |
|---|---|
| `Ctrl+Space` | Quick Search |
| `Ctrl+Shift+Space` | Apps |
| `Ctrl+Alt+N` | Tambah tugas atau pengingat (Quick Search mode Tugas) |
| `Ctrl+↑` | Overview |
| `Alt+Tab` | Pengalih ABRA (hanya saat Scene aktif) |
| `` Alt+` `` / `` Alt+Shift+` `` | Pindah ke jendela berikut / sebelumnya dari app yang sama |
| `Ctrl+Alt+H` | Sembunyikan (minimalkan) semua jendela app aktif |
| `Ctrl+Alt+M` | Minimalkan jendela aktif |
| `Ctrl+Alt+Q` | Tutup semua jendela app aktif dengan sopan (app boleh bertanya "simpan?") |
| `Esc` | Menutup overlay aktif |

Pintasan lain (screen corner) diatur di Pengaturan. Bila sebuah pintasan tidak bekerja, mungkin sudah dipakai program lain (tercatat di
`abra.log`).

## 13. Privasi dan keamanan

- **Tanpa telemetri.** ABRA tidak mengumpulkan atau mengirim data pemakaian.
- **Cek pembaruan.** Kira-kira tiap 12 jam ABRA membaca daftar rilis publik dari `api.github.com`; tidak ada data tentang Anda yang
  dikirim. Installer baru hanya diunduh bila Anda mengklik **Perbarui**. Matikan di Pengaturan → Umum → Tentang.
- **Pencarian lirik** untuk Visor: bila lagu tidak punya berkas `.lrc` lokal, ABRA bertanya ke
  [LRCLIB](https://lrclib.net) dengan judul, artis, album, dan durasi lagu saja, lalu menyimpan jawabannya di
  `%APPDATA%\ABRA\lyrics\cache`. Aktif bawaan; matikan di Pengaturan → Visor → Lirik → "Cari lirik online (LRCLIB)".
  Baris "Cari di web" di Quick Search hanya membuka browser Anda bila Anda memilihnya.
- **Data lokal.** Pengaturan, sematan, dan log disimpan di `%APPDATA%\ABRA`. Tidak ada data yang dikumpulkan.
- **Unduhan di Visor.** ABRA hanya membaca nama dan ukuran berkas sementara di folder Unduhan, antrean pemasangan Microsoft
  Store milik Windows, dan berkas status Steam (`appmanifest_*.acf`); isi berkas tidak dibuka dan tidak ada yang dikirim.
- **Wi-Fi dan Bluetooth.** Daftar jaringan dan perangkat dibaca dari Windows saat menunya dibuka dan tidak disimpan.
- **Kirim ke HP.** Selama sesi Kirim, ABRA membuka server web kecil sementara di alamat jaringan lokal laptop (port
  acak, tautan berisi kode acak). Hanya berkas yang Anda taruh yang bisa diunduh, hanya dari perangkat di jaringan
  lokal, dan servernya mati saat sesi berakhir. Tidak ada yang lewat internet atau server luar.
- **Kalender.** Kartu kalender di tab Timer membaca judul dan jam acara satu tanggal dari Kalender Windows (hanya baca)
  saat kartu itu terlihat; tidak ada yang disimpan atau dikirim.
- **Hook keyboard.** Saat Scene dan pengalih Alt+Tab aktif, ABRA memasang hook keyboard tingkat rendah semata-mata untuk
  menelan kombinasi `Alt+Tab` dan menampilkan pengalihnya. Ketikan tidak dicatat, disimpan, atau dikirim. Hook dilepas saat fitur
  dimatikan.
- **Hook mouse** dipakai singkat untuk menutup panel saat Anda mengeklik di luarnya.
- **Pintasan cepat** hanya memakai `RegisterHotKey` dan pesan jendela standar (WM_CLOSE, minimalkan); tidak ada yang disuntikkan ke app lain. Bisa dimatikan di Pengaturan → Umum.
- **Perintah sistem** (kunci layar, kalkulator, dsb.) dijalankan dari jalur absolut `System32`, bukan lewat PATH, sehingga tidak bisa dibajak oleh berkas dengan nama sama.
- **Memindahkan jendela.** Scene memindahkan jendela app lain ke luar layar dan mengembalikannya. Tidak ada isi jendela yang
  disimpan di disk selain daftar posisi sementara (`parked.txt`) agar jendela bisa dipulihkan.
- **Mulai otomatis** memakai tugas Task Scheduler pengguna bernama `ABRA Autostart` (tanpa hak Administrator) supaya ABRA langsung
  muncul setelah login; bila tugas tidak bisa dibuat, ABRA memakai nilai `ABRA` di
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Matikan dari Pengaturan ABRA.
- Perangkat lunak ini disediakan **apa adanya** tanpa jaminan (lihat [LICENSE](../LICENSE.md)). Kode sumbernya terbuka untuk
  diperiksa.

## 14. Pemecahan masalah

| Masalah | Solusi |
|---|---|
| Taskbar hilang setelah ABRA mati | Jalankan `abra.exe` lalu keluar normal, atau biarkan `abra-guard.exe` memulihkannya. Pastikan kedua berkas satu folder. |
| Ada jendela yang "hilang" | Kemungkinan diparkir Scene. Jalankan ABRA lagi (ia memulihkan jendela terparkir saat mulai), atau matikan Scene. |
| Pintasan tidak bekerja | Program lain mungkin memakai kombinasi yang sama; cek `abra.log`. |
| Menu app tidak muncul di Rail | App itu tidak memakai menu Win32 klasik, jadi menu standar dipakai. |
| Rail/Deck tidak terlihat di layar penuh | Itu bawaan: Rail, Visor, dan Deck bersembunyi saat video atau game layar penuh. Bila ingin tetap tampil, aktifkan Pengaturan → Umum → **Selalu di atas jendela layar penuh**. Game dengan layar penuh eksklusif tidak bisa ditimpa. |
| Deck tidak transparan | Efek transparansi Windows mati atau penghemat baterai aktif, jadi ABRA tampil solid. Pengaturan → Tampilan → Kaca menampilkan tombol untuk membuka pengaturan Windows yang terkait. |
| Task Manager tidak terbuka dari ABRA | Sudah diperbaiki di 0.5.0. |
| Ikon desktop tidak terlihat | Pengaturan → Umum → matikan **Sembunyikan ikon desktop saat ABRA berjalan**. Ikon kembali saat ABRA ditutup. |
| Ikon Deck terlihat berbeda dari aslinya | Matikan **Ikon seragam** di tab DECK dan setel **Gaya ikon** ke Bawaan. |
| Kursor tetap bulat setelah ABRA mati | Jalankan ABRA lagi dan keluar normal, atau biarkan `abra-guard.exe` memulihkannya; bisa juga dengan Pengaturan Windows → Bluetooth & perangkat → Mouse → Penunjuk mouse. |
| Melaporkan bug | Sertakan versi Windows, langkah mengulang, dan isi `abra.log` pada Issues proyek. |

## 15. Batasan yang diketahui

- Menyeret kartu Scene ke panggung belum tersedia (gunakan Shift-klik). Kartu hidup memakai Windows.Graphics.Capture, jadi
  memerlukan Windows 10 versi 1903 atau lebih baru (Windows 11 sudah memenuhi).
- App yang dijalankan sebagai Administrator tidak bisa dikelola Scene.
- Fokus, Hotspot, dan Mode Pesawat di Quick Panel hanya membuka halaman pengaturan Windows yang sesuai.
- Isi menu klasik yang diisi "malas" oleh app (misalnya daftar berkas terbaru) tidak tampil di Rail.
- "Paksa Keluar" membuka Task Manager.
- Diuji pada Windows 11 dengan satu layar utama.

## 16. Mencopot pemasangan

- **Installer:** Pengaturan Windows → Aplikasi → ABRA → Copot. Installer menghentikan ABRA, memulihkan taskbar, dan menghapus entri
  mulai otomatis.
- **Zip portabel:** matikan **Jalankan saat login Windows** di Pengaturan ABRA, keluar dari ABRA (taskbar kembali), lalu hapus
  foldernya.
- Opsional: hapus `%APPDATA%\ABRA` untuk membuang semua pengaturan.
