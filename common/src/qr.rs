//! Генератор QR-кодов (ISO/IEC 18004): байтовый режим, коррекция M,
//! версии 1-40 (до ~2331 байт). Свой, без зависимостей - перенос пароля и
//! синхронизация списка на другое устройство должны работать даже в
//! «апокалипсис». Проверяется round-trip тестом с внешним декодером
//! (dev-зависимость, в поставку не входит), см. tests/qr.rs.

/// максимум полезных байт (v40-M): (2334*8 - 4 - 16) / 8
pub const MAX_BYTES: usize = 2331;

/// (всего кодслов данных, EC на блок, блоков г1, данных в г1, блоков г2, данных в г2)
const V_TABLE: [(usize, usize, usize, usize, usize, usize); 40] = [
    (16, 10, 1, 16, 0, 0),      // v1
    (28, 16, 1, 28, 0, 0),      // v2
    (44, 26, 1, 44, 0, 0),      // v3
    (64, 18, 2, 32, 0, 0),      // v4
    (86, 24, 2, 43, 0, 0),      // v5
    (108, 16, 4, 27, 0, 0),     // v6
    (124, 18, 4, 31, 0, 0),     // v7
    (154, 22, 2, 38, 2, 39),    // v8
    (182, 22, 3, 36, 2, 37),    // v9
    (216, 26, 4, 43, 1, 44),    // v10
    (254, 30, 1, 50, 4, 51),    // v11
    (290, 22, 6, 36, 2, 37),    // v12
    (334, 22, 8, 37, 1, 38),    // v13
    (365, 24, 4, 40, 5, 41),    // v14
    (415, 24, 5, 41, 5, 42),    // v15
    (453, 28, 7, 45, 3, 46),    // v16
    (507, 28, 10, 46, 1, 47),   // v17
    (563, 26, 9, 43, 4, 44),    // v18
    (627, 26, 3, 44, 11, 45),   // v19
    (669, 26, 3, 41, 13, 42),   // v20
    (714, 26, 17, 42, 0, 0),    // v21
    (782, 28, 17, 46, 0, 0),    // v22
    (860, 28, 4, 47, 14, 48),   // v23
    (914, 28, 6, 45, 14, 46),   // v24
    (1000, 28, 8, 47, 13, 48),  // v25
    (1062, 28, 19, 46, 4, 47),  // v26
    (1128, 28, 22, 45, 3, 46),  // v27
    (1193, 28, 3, 45, 23, 46),  // v28
    (1267, 28, 21, 45, 7, 46),  // v29
    (1373, 28, 19, 47, 10, 48), // v30
    (1455, 28, 2, 46, 29, 47),  // v31
    (1541, 28, 10, 46, 23, 47), // v32
    (1631, 28, 14, 46, 21, 47), // v33
    (1725, 28, 14, 46, 23, 47), // v34
    (1812, 28, 12, 47, 26, 48), // v35
    (1914, 28, 6, 47, 34, 48),  // v36
    (1992, 28, 29, 46, 14, 47), // v37
    (2102, 28, 13, 46, 32, 47), // v38
    (2216, 28, 40, 47, 7, 48),  // v39
    (2334, 28, 18, 47, 31, 48), // v40
];

/// Центры выравнивающих узоров по версиям (v2..=v40).
const ALIGN: [&[usize]; 39] = [
    &[6, 18],                        // v2
    &[6, 22],                        // v3
    &[6, 26],                        // v4
    &[6, 30],                        // v5
    &[6, 34],                        // v6
    &[6, 22, 38],                    // v7
    &[6, 24, 42],                    // v8
    &[6, 26, 46],                    // v9
    &[6, 28, 50],                    // v10
    &[6, 30, 54],                    // v11
    &[6, 32, 58],                    // v12
    &[6, 34, 62],                    // v13
    &[6, 26, 46, 66],                // v14
    &[6, 26, 48, 70],                // v15
    &[6, 26, 50, 74],                // v16
    &[6, 30, 54, 78],                // v17
    &[6, 30, 56, 82],                // v18
    &[6, 30, 58, 86],                // v19
    &[6, 34, 62, 90],                // v20
    &[6, 28, 50, 72, 94],            // v21
    &[6, 26, 50, 74, 98],            // v22
    &[6, 30, 54, 78, 102],           // v23
    &[6, 28, 54, 80, 106],           // v24
    &[6, 32, 58, 84, 110],           // v25
    &[6, 30, 58, 86, 114],           // v26
    &[6, 34, 62, 90, 118],           // v27
    &[6, 26, 50, 74, 98, 122],       // v28
    &[6, 30, 54, 78, 102, 126],      // v29
    &[6, 26, 52, 78, 104, 130],      // v30
    &[6, 30, 56, 82, 108, 134],      // v31
    &[6, 34, 60, 86, 112, 138],      // v32
    &[6, 30, 58, 86, 114, 142],      // v33
    &[6, 34, 62, 90, 118, 146],      // v34
    &[6, 30, 54, 78, 102, 126, 150], // v35
    &[6, 24, 50, 76, 102, 128, 154], // v36
    &[6, 28, 54, 80, 106, 132, 158], // v37
    &[6, 32, 58, 84, 110, 136, 162], // v38
    &[6, 26, 54, 82, 110, 138, 166], // v39
    &[6, 30, 58, 86, 114, 142, 170], // v40
];

// GF(256), примитивный полином 0x11D

fn gf_tables() -> ([u8; 256], [u8; 512]) {
    let mut log = [0u8; 256];
    let mut exp = [0u8; 512];
    let mut x: u16 = 1;
    for i in 0..255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
    }
    for i in 255..512 {
        exp[i] = exp[i - 255];
    }
    (log, exp)
}

/// Кодслова Рида-Соломона для блока данных.
fn rs_ec(data: &[u8], ec_len: usize) -> Vec<u8> {
    let (log, exp) = gf_tables();
    // генераторный полином: произведение (x - α^i), i = 0..ec_len
    let mut gen = vec![0u8; ec_len + 1];
    gen[0] = 1;
    for i in 0..ec_len {
        let mut next = vec![0u8; ec_len + 1];
        for j in 0..=i {
            // домножаем полином на (x + α^i)
            next[j] ^= gen[j]; // множитель x
            if gen[j] != 0 {
                next[j + 1] ^= exp[log[gen[j] as usize] as usize + i];
            }
        }
        gen = next;
    }
    // делим data * x^ec_len на gen
    let mut rem = vec![0u8; ec_len];
    for &d in data {
        let factor = d ^ rem[0];
        rem.remove(0);
        rem.push(0);
        if factor != 0 {
            let lf = log[factor as usize] as usize;
            for (r, &g) in rem.iter_mut().zip(gen[1..].iter()) {
                if g != 0 {
                    *r ^= exp[lf + log[g as usize] as usize];
                }
            }
        }
    }
    rem
}

// BCH для формата и версии

fn format_bits(mask: u8) -> u16 {
    // уровень M = 0b00
    let data: u16 = ((0b00u16) << 3) | mask as u16;
    let mut v = (data as u32) << 10;
    let g: u32 = 0x537;
    for i in (10..15).rev() {
        if v & (1 << i) != 0 {
            v ^= g << (i - 10);
        }
    }
    ((((data as u32) << 10) | v) as u16) ^ 0x5412
}

fn version_bits(version: usize) -> u32 {
    let mut v = (version as u32) << 12;
    let g: u32 = 0x1F25;
    for i in (12..18).rev() {
        if v & (1 << i) != 0 {
            v ^= g << (i - 12);
        }
    }
    ((version as u32) << 12) | v
}

// Кодирование данных

fn choose_version(len: usize) -> Option<usize> {
    for v in 1..=40usize {
        let data_cw = V_TABLE[v - 1].0;
        // индикатор длины в байтовом режиме: 8 бит для v1-9, иначе 16
        let cnt_bits = if v <= 9 { 8 } else { 16 };
        if 4 + cnt_bits + 8 * len <= data_cw * 8 {
            return Some(v);
        }
    }
    None
}

struct BitWriter {
    bytes: Vec<u8>,
    bit: usize,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter { bytes: Vec::new(), bit: 0 }
    }
    fn push(&mut self, value: u32, n: usize) {
        for i in (0..n).rev() {
            if self.bit % 8 == 0 {
                self.bytes.push(0);
            }
            if value & (1 << i) != 0 {
                let idx = self.bit / 8;
                self.bytes[idx] |= 1 << (7 - self.bit % 8);
            }
            self.bit += 1;
        }
    }
}

fn encode_codewords(data: &[u8], version: usize) -> Vec<u8> {
    let (data_cw, ec_len, g1n, g1d, g2n, g2d) = V_TABLE[version - 1];
    let mut w = BitWriter::new();
    w.push(0b0100, 4); // байтовый режим
    w.push(data.len() as u32, if version <= 9 { 8 } else { 16 });
    for &b in data {
        w.push(b as u32, 8);
    }
    // терминатор и добивка до целого байта
    let used = w.bit;
    let cap = data_cw * 8;
    w.push(0, core::cmp::min(4, cap - used));
    if w.bit % 8 != 0 {
        w.push(0, 8 - w.bit % 8);
    }
    // байты-заполнители
    let mut pad = [0xEC, 0x11].iter().cycle();
    while w.bytes.len() < data_cw {
        w.bytes.push(*pad.next().unwrap());
    }

    // блоки, EC, перемежение
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut pos = 0;
    for _ in 0..g1n {
        blocks.push(&w.bytes[pos..pos + g1d]);
        pos += g1d;
    }
    for _ in 0..g2n {
        blocks.push(&w.bytes[pos..pos + g2d]);
        pos += g2d;
    }
    let ecs: Vec<Vec<u8>> = blocks.iter().map(|b| rs_ec(b, ec_len)).collect();

    let max_d = blocks.iter().map(|b| b.len()).max().unwrap();
    let mut out = Vec::with_capacity(data_cw + ec_len * blocks.len());
    for i in 0..max_d {
        for b in &blocks {
            if i < b.len() {
                out.push(b[i]);
            }
        }
    }
    for i in 0..ec_len {
        for e in &ecs {
            out.push(e[i]);
        }
    }
    out
}

// Матрица

#[derive(Clone)]
struct Grid {
    size: usize,
    dark: Vec<bool>,
    func: Vec<bool>, // служебные модули: маска и данные их не трогают
}

impl Grid {
    fn new(size: usize) -> Self {
        Grid { size, dark: vec![false; size * size], func: vec![false; size * size] }
    }
    fn set(&mut self, r: usize, c: usize, dark: bool) {
        let i = r * self.size + c;
        self.dark[i] = dark;
        self.func[i] = true;
    }
    fn get(&self, r: usize, c: usize) -> bool {
        self.dark[r * self.size + c]
    }
    fn is_func(&self, r: usize, c: usize) -> bool {
        self.func[r * self.size + c]
    }
}

fn place_finder(g: &mut Grid, r0: i32, c0: i32) {
    for dr in -1..=7i32 {
        for dc in -1..=7i32 {
            let (r, c) = (r0 + dr, c0 + dc);
            if r < 0 || c < 0 || r >= g.size as i32 || c >= g.size as i32 {
                continue;
            }
            let dark = (0..=6).contains(&dr)
                && (0..=6).contains(&dc)
                && (dr == 0 || dr == 6 || dc == 0 || dc == 6 || ((2..=4).contains(&dr) && (2..=4).contains(&dc)));
            g.set(r as usize, c as usize, dark);
        }
    }
}

fn build_base(version: usize) -> Grid {
    let size = 17 + 4 * version;
    let mut g = Grid::new(size);

    place_finder(&mut g, 0, 0);
    place_finder(&mut g, 0, size as i32 - 7);
    place_finder(&mut g, size as i32 - 7, 0);

    // синхрополосы
    for i in 8..size - 8 {
        if !g.is_func(6, i) {
            g.set(6, i, i % 2 == 0);
        }
        if !g.is_func(i, 6) {
            g.set(i, 6, i % 2 == 0);
        }
    }

    // выравнивающие узоры
    if version >= 2 {
        let centers = ALIGN[version - 2];
        let last = size - 7;
        for &r in centers {
            for &c in centers {
                // пропускаем только узоры, попадающие на поисковые;
                // синхрополосу выравнивающий узор перекрывать можно
                if (r == 6 && c == 6) || (r == 6 && c == last) || (r == last && c == 6) {
                    continue;
                }
                for dr in -2..=2i32 {
                    for dc in -2..=2i32 {
                        let dark = dr.abs() == 2 || dc.abs() == 2 || (dr == 0 && dc == 0);
                        g.set((r as i32 + dr) as usize, (c as i32 + dc) as usize, dark);
                    }
                }
            }
        }
    }

    // тёмный модуль
    g.set(size - 8, 8, true);

    // резерв под формат, значения впишутся при выборе маски
    for i in 0..9 {
        if !g.is_func(8, i) {
            g.set(8, i, false);
        }
        if !g.is_func(i, 8) {
            g.set(i, 8, false);
        }
    }
    for i in 0..8 {
        if !g.is_func(8, size - 1 - i) {
            g.set(8, size - 1 - i, false);
        }
        if !g.is_func(size - 1 - i, 8) {
            g.set(size - 1 - i, 8, false);
        }
    }

    // информация о версии (v >= 7)
    if version >= 7 {
        let bits = version_bits(version);
        for i in 0..18 {
            let dark = bits & (1 << i) != 0;
            let (a, b) = (i / 3, size - 11 + i % 3);
            g.set(a, b, dark); // правый верхний блок
            g.set(b, a, dark); // левый нижний
        }
    }
    g
}

fn place_data(g: &mut Grid, codewords: &[u8]) {
    let size = g.size;
    let mut bit_idx = 0usize;
    let total_bits = codewords.len() * 8;
    let mut upward = true;
    let mut col = size as i32 - 1;
    while col > 0 {
        if col == 6 {
            col -= 1; // пропускаем столбец синхрополосы
        }
        let rows: Vec<usize> = if upward { (0..size).rev().collect() } else { (0..size).collect() };
        for r in rows {
            for c in [col as usize, col as usize - 1] {
                if g.is_func(r, c) {
                    continue;
                }
                let dark = if bit_idx < total_bits {
                    codewords[bit_idx / 8] & (1 << (7 - bit_idx % 8)) != 0
                } else {
                    false // остаток добиваем нулями
                };
                g.dark[r * size + c] = dark;
                bit_idx += 1;
            }
        }
        upward = !upward;
        col -= 2;
    }
}

fn mask_predicate(mask: u8, r: usize, c: usize) -> bool {
    match mask {
        0 => (r + c) % 2 == 0,
        1 => r % 2 == 0,
        2 => c % 3 == 0,
        3 => (r + c) % 3 == 0,
        4 => (r / 2 + c / 3) % 2 == 0,
        5 => (r * c) % 2 + (r * c) % 3 == 0,
        6 => ((r * c) % 2 + (r * c) % 3) % 2 == 0,
        _ => ((r + c) % 2 + (r * c) % 3) % 2 == 0,
    }
}

fn write_format(g: &mut Grid, mask: u8) {
    let f = format_bits(mask) as u32;
    let size = g.size;
    let bit = |i: usize| f & (1 << (14 - i)) != 0; // i=0 - старший бит
    // копия 1: вокруг левого верхнего узора
    let coords1 = [
        (8usize, 0usize), (8, 1), (8, 2), (8, 3), (8, 4), (8, 5), (8, 7), (8, 8),
        (7, 8), (5, 8), (4, 8), (3, 8), (2, 8), (1, 8), (0, 8),
    ];
    for (i, &(r, c)) in coords1.iter().enumerate() {
        g.dark[r * size + c] = bit(i);
    }
    // копия 2: снизу слева и справа сверху
    for i in 0..7 {
        g.dark[(size - 1 - i) * size + 8] = bit(i);
    }
    for i in 7..15 {
        g.dark[8 * size + (size - 15 + i)] = bit(i);
    }
}

fn penalty(g: &Grid) -> u32 {
    let size = g.size;
    let mut score = 0u32;
    // N1: серии одного цвета длиной >= 5
    for line in 0..size {
        let (mut run_r, mut run_c) = (1u32, 1u32);
        for i in 1..size {
            if g.get(line, i) == g.get(line, i - 1) {
                run_r += 1;
            } else {
                if run_r >= 5 { score += run_r - 2; }
                run_r = 1;
            }
            if g.get(i, line) == g.get(i - 1, line) {
                run_c += 1;
            } else {
                if run_c >= 5 { score += run_c - 2; }
                run_c = 1;
            }
        }
        if run_r >= 5 { score += run_r - 2; }
        if run_c >= 5 { score += run_c - 2; }
    }
    // N2: блоки 2x2
    for r in 0..size - 1 {
        for c in 0..size - 1 {
            let v = g.get(r, c);
            if v == g.get(r, c + 1) && v == g.get(r + 1, c) && v == g.get(r + 1, c + 1) {
                score += 3;
            }
        }
    }
    // N3: узор 1011101 с 0000 по одну из сторон
    let pat = [true, false, true, true, true, false, true];
    let check = |get: &dyn Fn(usize) -> bool, len: usize| -> u32 {
        let mut s = 0;
        for start in 0..len.saturating_sub(6) {
            if (0..7).all(|i| get(start + i) == pat[i]) {
                let before = start >= 4 && (start - 4..start).all(|i| !get(i));
                let after = start + 11 <= len && (start + 7..start + 11).all(|i| !get(i));
                if before || after {
                    s += 40;
                }
            }
        }
        s
    };
    for line in 0..size {
        score += check(&|i| g.get(line, i), size);
        score += check(&|i| g.get(i, line), size);
    }
    // N4: доля тёмных модулей
    let dark = g.dark.iter().filter(|&&d| d).count() as i32;
    let pct = dark * 100 / (size * size) as i32;
    score += ((pct - 50).abs() / 5) as u32 * 10;
    score
}

/// Матрица QR: true = тёмный модуль. Ошибка, если данные не влезают (см. MAX_BYTES).
pub fn matrix(data: &[u8]) -> Result<Vec<Vec<bool>>, String> {
    let version = choose_version(data.len())
        .ok_or_else(|| format!("слишком длинно для QR: {} байт (максимум {})", data.len(), MAX_BYTES))?;
    let codewords = encode_codewords(data, version);
    let base = {
        let mut g = build_base(version);
        place_data(&mut g, &codewords);
        g
    };

    let mut best: Option<(u32, Grid)> = None;
    for mask in 0u8..8 {
        let mut g = base.clone();
        for r in 0..g.size {
            for c in 0..g.size {
                if !g.is_func(r, c) && mask_predicate(mask, r, c) {
                    let i = r * g.size + c;
                    g.dark[i] = !g.dark[i];
                }
            }
        }
        write_format(&mut g, mask);
        let p = penalty(&g);
        if best.as_ref().map_or(true, |(bp, _)| p < *bp) {
            best = Some((p, g));
        }
    }
    let (_, g) = best.unwrap();
    let mut out = vec![vec![false; g.size]; g.size];
    for r in 0..g.size {
        for c in 0..g.size {
            out[r][c] = g.get(r, c);
        }
    }
    Ok(out)
}

/// SVG в палитре «Чернил»: тёмные модули на бумажном фоне, тихая зона в 4 модуля.
pub fn to_svg(text: &str) -> Result<String, String> {
    let m = matrix(text.as_bytes())?;
    let size = m.len();
    let total = size + 8; // по 4 модуля тихой зоны с каждой стороны
    let mut path = String::new();
    for (r, row) in m.iter().enumerate() {
        for (c, &dark) in row.iter().enumerate() {
            if dark {
                path.push_str(&format!("M{} {}h1v1h-1z", c + 4, r + 4));
            }
        }
    }
    Ok(format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {t} {t}\" ",
            "shape-rendering=\"crispEdges\">",
            "<rect width=\"{t}\" height=\"{t}\" fill=\"#F5F0E8\"/>",
            "<path d=\"{p}\" fill=\"#141210\"/></svg>"
        ),
        t = total,
        p = path
    ))
}
