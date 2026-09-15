// 第十二幕: Slint做自己的电子表格 — 完整功能
// 参考 yehun-slint StringGrid: 列宽调整 + 列排序 + 动态选区 + 文件加载

slint::include_modules!();

use calamine::{open_workbook, DataType, Reader, Xlsx};
use slint::{Model, ModelRc, SharedString, VecModel};
use slint_file_picker::{pick_file, FileFilter, PickResult};
use std::cmp::{max, min};
use std::path::Path;
use std::rc::Rc;

// ===== 数据结构辅助 =====

fn headers_from_row(row: &[String]) -> Vec<GridHeader> {
    row.iter()
        .map(|title| GridHeader {
            title: title.into(),
            align: 0,
            sortable: true,
        })
        .collect()
}

fn grid_rows_from_data(data: &[Vec<String>]) -> Vec<GridRow> {
    data.iter()
        .map(|row| {
            let values: Vec<SharedString> = row.iter().map(|s| s.as_str().into()).collect();
            GridRow {
                value: ModelRc::from(Rc::new(VecModel::from(values))),
            }
        })
        .collect()
}

// ===== CSV 解析 =====

fn parse_csv(path: &Path) -> Result<(Vec<GridHeader>, Vec<GridRow>), String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("打开 CSV 失败: {e}"))?;

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut col_count = 0;

    for result in reader.records() {
        let record = result.map_err(|e| format!("解析 CSV 行失败: {e}"))?;
        let row: Vec<String> = record.iter().map(|s| s.to_string()).collect();
        if row.len() > col_count {
            col_count = row.len();
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Err("CSV 文件为空".into());
    }

    let header_titles = {
        let first = &rows[0];
        let mut titles: Vec<String> = first.clone();
        while titles.len() < col_count {
            titles.push(format!("列{}", titles.len() + 1));
        }
        titles
    };

    let headers = headers_from_row(&header_titles);
    let data_rows: Vec<Vec<String>> = rows.into_iter().skip(1).collect();
    let grid_rows = grid_rows_from_data(&data_rows);

    Ok((headers, grid_rows))
}

// ===== XLSX/XLS 解析 =====

fn parse_excel(path: &Path) -> Result<(Vec<GridHeader>, Vec<GridRow>), String> {
    let mut workbook = open_workbook::<Xlsx<_>, _>(path)
        .map_err(|e| format!("打开 Excel 失败: {e:?}"))?;

    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err("Excel 文件无工作表".into());
    }

    let sheet = workbook
        .worksheet_range(&sheet_names[0])
        .map_err(|e| format!("读取工作表失败: {e:?}"))?;

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut col_count = 0;

    for row in sheet.rows() {
        let row_data: Vec<String> = row
            .iter()
            .map(|cell| {
                if cell.is_empty() {
                    String::new()
                } else if cell.is_string() {
                    cell.get_string().unwrap_or_default().to_string()
                } else if cell.is_int() {
                    cell.get_int().map(|v| v.to_string()).unwrap_or_default()
                } else if cell.is_float() {
                    let f = cell.get_float().unwrap_or(0.0);
                    if f == f.floor() && f.abs() < 1e15 {
                        (f as i64).to_string()
                    } else {
                        f.to_string()
                    }
                } else if cell.is_bool() {
                    cell.get_bool().map(|v| v.to_string()).unwrap_or_default()
                } else {
                    cell.as_string().unwrap_or_default()
                }
            })
            .collect();
        if row_data.len() > col_count {
            col_count = row_data.len();
        }
        rows.push(row_data);
    }

    if rows.is_empty() {
        return Err("工作表为空".into());
    }

    let header_titles = rows.first().cloned().unwrap_or_default();
    let headers = headers_from_row(&header_titles);
    let data_rows: Vec<Vec<String>> = rows.into_iter().skip(1).collect();
    let grid_rows = grid_rows_from_data(&data_rows);

    Ok((headers, grid_rows))
}

// ===== 通用文件加载 =====

fn load_file(path: &Path) -> Result<(Vec<GridHeader>, Vec<GridRow>), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" => parse_csv(path),
        "xlsx" | "xls" | "xlsm" => parse_excel(path),
        _ => Err(format!("不支持的文件格式: .{ext}")),
    }
}

// ===== 列宽前缀计算 =====

/// 根据各列宽度计算前缀数组 (col-x[c] = c列左边界)
fn compute_col_x(widths: &[f32]) -> Vec<f32> {
    let mut col_x = vec![0.0f32];
    for w in widths {
        col_x.push(col_x.last().unwrap() + w);
    }
    col_x
}

/// 估算文本像素宽度 (简单估算: 中文字符 14px, 英文字符 8px)
fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    let scale = font_size / 14.0;
    let mut width = 0.0f32;
    for ch in text.chars() {
        if ch as u32 > 0x7F {
            width += 14.0 * scale;
        } else {
            width += 8.0 * scale;
        }
    }
    width.max(24.0)
}

// ===== 初始化辅助 =====

fn init_spreadsheet(main_window: &MainWindow) {
    let model = main_window.global::<SpreadsheetModel>();

    // 列头配置 (新增 sortable 字段)
    let headers = vec![
        GridHeader { title: "姓名".into(), align: 0, sortable: true },
        GridHeader { title: "年龄".into(), align: 1, sortable: true },
        GridHeader { title: "城市".into(), align: 0, sortable: true },
        GridHeader { title: "部门".into(), align: 0, sortable: true },
        GridHeader { title: "薪资".into(), align: 2, sortable: true },
        GridHeader { title: "入职年份".into(), align: 1, sortable: true },
        GridHeader { title: "邮箱".into(), align: 0, sortable: true },
        GridHeader { title: "电话".into(), align: 0, sortable: true },
    ];
    let headers_model: VecModel<GridHeader> = VecModel::from(headers);
    model.set_headers(ModelRc::from(Rc::new(headers_model)));

    // 行数据
    let data: Vec<Vec<&str>> = vec![
        vec!["张三", "28", "北京", "研发", "25000", "2020", "zhang@example.com", "13800138001"],
        vec!["李四", "32", "上海", "产品", "30000", "2018", "li@example.com", "13800138002"],
        vec!["王五", "25", "深圳", "设计", "20000", "2022", "wang@example.com", "13800138003"],
        vec!["赵六", "35", "广州", "研发", "35000", "2016", "zhao@example.com", "13800138004"],
        vec!["钱七", "29", "杭州", "运营", "22000", "2021", "qian@example.com", "13800138005"],
        vec!["孙八", "27", "成都", "研发", "24000", "2021", "sun@example.com", "13800138006"],
        vec!["周九", "31", "武汉", "产品", "28000", "2019", "zhou@example.com", "13800138007"],
        vec!["吴十", "26", "南京", "设计", "21000", "2023", "wu@example.com", "13800138008"],
    ];

    let rows: Vec<GridRow> = data
        .iter()
        .map(|row| {
            let values: Vec<SharedString> = row.iter().map(|s| (*s).into()).collect();
            GridRow {
                value: ModelRc::from(Rc::new(VecModel::from(values))),
            }
        })
        .collect();
    let rows_model = Rc::new(VecModel::from(rows));
    model.set_rows(ModelRc::from(rows_model));

    // 初始化列宽前缀
    let col_widths: Vec<f32> = vec![92.0; 8];
    let col_x = compute_col_x(&col_widths);
    model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));

    // 保存原始顺序
    save_original_rows(&model);

    model.set_status("就绪 — 共 8 行 8 列 | 列头: 单击排序/拖拽调宽/双击自适应".into());
}

// ===== 回调设置 =====

/// 设置 load-data 回调
fn setup_load_data(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_load_data(move || {
        let Some(win) = win_weak.upgrade() else { return };
        win.global::<SpreadsheetModel>()
            .set_status("正在打开文件选择器...".into());

        let win_weak = win_weak.clone();
        pick_file(
            vec![
                FileFilter::new("表格文件")
                    .extension("xlsx").extension("xls").extension("csv")
                    .mime("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
                    .mime("application/vnd.ms-excel")
                    .mime("text/csv"),
                FileFilter::new("Excel 文件")
                    .extension("xlsx").extension("xls")
                    .mime("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
                    .mime("application/vnd.ms-excel"),
                FileFilter::new("CSV 文件")
                    .extension("csv")
                    .mime("text/csv"),
                FileFilter::new("所有文件").mime("*/*"),
            ],
            move |result| {
                let payload: Result<(String, Vec<GridHeader>, Vec<Vec<String>>), String> =
                    match result {
                        PickResult::Picked(platform_path) => {
                            let path_str = platform_path.to_string();
                            let path = Path::new(&path_str);
                            match load_file(path) {
                                Ok((headers, rows)) => {
                                    let raw_rows: Vec<Vec<String>> = rows
                                        .iter()
                                        .map(|row| {
                                            let count = row.value.row_count();
                                            (0..count)
                                                .map(|i| row.value.row_data(i).unwrap().to_string())
                                                .collect()
                                        })
                                        .collect();
                                    Ok((path_str, headers, raw_rows))
                                }
                                Err(e) => Err(format!("解析失败: {e}")),
                            }
                        }
                        PickResult::Cancelled => return,
                        PickResult::Error(e) => Err(format!("选择失败: {e}")),
                    };

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(win) = win_weak.upgrade() else { return };
                    let model = win.global::<SpreadsheetModel>();
                    match payload {
                        Ok((path_str, headers, raw_rows)) => {
                            let col_count = headers.len().max(1);
                            let headers_model = Rc::new(VecModel::from(headers));
                            model.set_headers(ModelRc::from(headers_model));
                            let rows: Vec<GridRow> = raw_rows
                                .iter()
                                .map(|row| {
                                    let values: Vec<SharedString> =
                                        row.iter().map(|s| s.as_str().into()).collect();
                                    GridRow {
                                        value: ModelRc::from(Rc::new(VecModel::from(values))),
                                    }
                                })
                                .collect();
                            let rows_model = Rc::new(VecModel::from(rows));
                            model.set_rows(ModelRc::from(rows_model));
                            // 重置列宽
                            let col_widths: Vec<f32> = vec![92.0; col_count];
                            let col_x = compute_col_x(&col_widths);
                            model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));

                            // 保存原始顺序
                            save_original_rows(&model);

                            model.set_status(format!("已加载: {path_str} ({} 行 {} 列)", raw_rows.len(), col_count).into());
                        }
                        Err(e) => model.set_status(e.into()),
                    }
                });
            },
        );
    });
}

/// 设置 sort 回调 (单击列头排序, 三态循环)
/// 从 model 提取所有行为 Vec<Vec<String>>
fn extract_rows(model: &SpreadsheetModel) -> Vec<Vec<String>> {
    let row_count = model.get_row_count() as usize;
    let mut rows = Vec::with_capacity(row_count);
    for r in 0..row_count {
        let row_data = model.get_rows().row_data(r).unwrap();
        let col_count = row_data.value.row_count();
        let mut row = Vec::with_capacity(col_count);
        for i in 0..col_count {
            row.push(row_data.value.row_data(i).unwrap().to_string());
        }
        rows.push(row);
    }
    rows
}

/// 将 Vec<Vec<String>> 写回 model
fn write_rows(model: &SpreadsheetModel, rows: &[Vec<String>]) {
    let grid_rows: Vec<GridRow> = rows
        .iter()
        .map(|row| {
            let values: Vec<SharedString> = row.iter().map(|s| s.as_str().into()).collect();
            GridRow {
                value: ModelRc::from(Rc::new(VecModel::from(values))),
            }
        })
        .collect();
    let rows_model = Rc::new(VecModel::from(grid_rows));
    model.set_rows(ModelRc::from(rows_model));
}

use std::cell::RefCell;

// 保存原始数据顺序 (用于恢复)
thread_local! {
    static ORIGINAL_ROWS: RefCell<Vec<Vec<String>>> = RefCell::new(Vec::new());
    static PENDING_CLICK_COL: RefCell<i32> = RefCell::new(-1);
}

/// 保存当前数据为原始顺序
fn save_original_rows(model: &SpreadsheetModel) {
    ORIGINAL_ROWS.with(|o| *o.borrow_mut() = extract_rows(model));
}

fn do_sort(model: &SpreadsheetModel, col: i32) {
    let col_count = model.get_col_count();
    if col < 0 || col >= col_count { return; }

    // 三态循环: 原始(0) → 倒序(2) → 正序(1) → 原始(0)
    let new_order = if model.get_sort_col() == col {
        match model.get_sort_order() {
            0 => 2,
            2 => 1,
            _ => 0,
        }
    } else {
        2
    };

    model.set_sort_col(col);
    model.set_sort_order(new_order);

    if new_order == 0 {
        ORIGINAL_ROWS.with(|o| {
            let original = o.borrow();
            if !original.is_empty() {
                write_rows(model, &original);
                model.set_status("已恢复原始顺序".into());
            }
        });
        model.set_sort_col(-1);
        model.set_sort_order(0);
        return;
    }

    save_original_rows(model);

    let mut rows = extract_rows(model);
    let col_idx = col as usize;
    rows.sort_by(|a, b| {
        let a_val = a.get(col_idx).map(|s| s.as_str()).unwrap_or("");
        let b_val = b.get(col_idx).map(|s| s.as_str()).unwrap_or("");
        let cmp = match (a_val.parse::<f64>(), b_val.parse::<f64>()) {
            (Ok(a_num), Ok(b_num)) => a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal),
            _ => a_val.cmp(b_val),
        };
        if new_order == 2 { cmp.reverse() } else { cmp }
    });

    write_rows(model, &rows);
    model.set_status(format!("按第 {} 列{}排序 ({} 行)", col + 1,
        if new_order == 1 { "正序" } else { "倒序" }, rows.len()).into());
}

/// 设置 copy 回调 (复制选区到剪贴板)
fn setup_copy(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_copy(move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<SpreadsheetModel>();

        let row0 = model.get_sel_row0() as usize;
        let row1 = model.get_sel_row1() as usize;
        let col0 = model.get_sel_col0() as usize;
        let col1 = model.get_sel_col1() as usize;

        let mut text = String::new();
        for r in row0..=row1 {
            if r >= model.get_row_count() as usize { break; }
            for c in col0..=col1 {
                if c >= model.get_col_count() as usize { break; }
                if c > col0 { text.push('\t'); }
                text.push_str(&model.invoke_cell_text(r as i32, c as i32));
            }
            if r < row1 { text.push('\n'); }
        }

        if !text.is_empty() {
            #[cfg(not(target_os = "android"))]
            {
                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        if clipboard.set_text(text.clone()).is_ok() {
                            model.set_status(format!("已复制 {} 个单元格到剪贴板", (row1 - row0 + 1) * (col1 - col0 + 1)).into());
                        } else {
                            model.set_status("复制失败".into());
                        }
                    }
                    Err(e) => {
                        model.set_status(format!("剪贴板不可用: {e}").into());
                    }
                }
            }
            #[cfg(target_os = "android")]
            {
                model.set_status("Android 复制功能待实现".into());
            }
        } else {
            model.set_status("无选区可复制".into());
        }
    });
}

/// 设置 auto-fit-selection 回调 (自适应选中列宽)
fn setup_auto_fit_selection(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_auto_fit_selection(move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<SpreadsheetModel>();

        let col0 = model.get_sel_col0();
        let col1 = model.get_sel_col1();
        if col0 < 0 || col1 < 0 { return; }

        let col_start = col0 as i32;
        let col_end = col1 as i32;
        let col_count = model.get_col_count();
        let row_count = model.get_row_count();

        let mut widths: Vec<f32> = (0..col_count).map(|c| model.invoke_col_width_of(c) as f32).collect();

        for col in col_start..=col_end {
            if col < 0 || col >= col_count { continue; }
            let mut max_width = model.get_min_col_width() as f32;
            for r in 0..row_count {
                let text = model.invoke_cell_text(r, col);
                let w = estimate_text_width(&text, 14.0);
                if w > max_width { max_width = w; }
            }
            widths[col as usize] = max_width + 16.0;
        }

        let col_x: Vec<f32> = compute_col_x(&widths);
        model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));

        model.set_status(format!("已自适应 {} 列宽度", (col_end - col_start + 1).max(0)).into());
    });
}

/// 设置 single-click / double-click 回调
fn setup_click_handlers(main_window: &MainWindow) {
    // single-click: 延时 250ms 后执行排序
    let win_weak_sort = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_single_click(move |col| {
        let _win = win_weak_sort.upgrade();
        if _win.is_none() { return; }
        PENDING_CLICK_COL.with(|p| *p.borrow_mut() = col as i32);

        let win_weak_delayed = win_weak_sort.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(250), move || {
            let Some(win) = win_weak_delayed.upgrade() else { return };
            let model = win.global::<SpreadsheetModel>();
            let pending = PENDING_CLICK_COL.with(|p| *p.borrow());
            if pending >= 0 {
                PENDING_CLICK_COL.with(|p| *p.borrow_mut() = -1);
                do_sort(&model, pending);
            }
        });
    });

    // double-click: 取消待处理的单击, 执行自适应列宽
    let win_weak_fit = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_double_click(move |col| {
        let Some(win) = win_weak_fit.upgrade() else { return };
        PENDING_CLICK_COL.with(|p| *p.borrow_mut() = -1);

        let model = win.global::<SpreadsheetModel>();
        let col = col as i32;
        let col_count = model.get_col_count();
        if col < 0 || col >= col_count { return; }

        let mut max_width = model.get_min_col_width() as f32;
        let row_count = model.get_row_count();
        for r in 0..row_count {
            let text = model.invoke_cell_text(r, col);
            let w = estimate_text_width(&text, 14.0);
            if w > max_width { max_width = w; }
        }
        max_width += 16.0;

        let mut widths: Vec<f32> = (0..col_count).map(|c| model.invoke_col_width_of(c) as f32).collect();
        widths[col as usize] = max_width;
        let col_x: Vec<f32> = compute_col_x(&widths);
        model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));

        model.set_status(format!("第 {} 列自适应宽度: {}px", col + 1, max_width as i32).into());
    });
}

/// 设置 resize-column 回调
fn setup_resize_column(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_resize_column(move |col, new_width| {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<SpreadsheetModel>();

        let col_count = model.get_col_count();
        if col < 0 || col >= col_count { return; }

        // 读取当前列宽
        let mut widths = Vec::with_capacity(col_count as usize);
        for c in 0..col_count {
            widths.push(model.invoke_col_width_of(c) as f32);
        }

        // 更新指定列宽
        widths[col as usize] = new_width;

        // 重新计算前缀数组
        let col_x = compute_col_x(&widths);
        model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));
    });
}

/// 设置 auto-fit-column 回调 (双击列头右缘)
fn setup_auto_fit_column(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_auto_fit_column(move |col, font_size| {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<SpreadsheetModel>();

        let col_count = model.get_col_count();
        let row_count = model.get_row_count();
        if col < 0 || col >= col_count { return; }

        // 计算该列所有文本的最大宽度
        let mut max_width = model.get_min_col_width() as f32;

        for r in 0..row_count {
            let text = model.invoke_cell_text(r, col);
            let w = estimate_text_width(&text, font_size);
            if w > max_width {
                max_width = w;
            }
        }

        // 加上内边距
        max_width += 16.0;

        // 读取当前列宽并更新
        let mut widths = Vec::with_capacity(col_count as usize);
        for c in 0..col_count {
            widths.push(model.invoke_col_width_of(c) as f32);
        }
        widths[col as usize] = max_width;

        let col_x = compute_col_x(&widths);
        model.set_col_x(ModelRc::from(Rc::new(VecModel::from(col_x))));

        model.set_status(format!("第 {} 列自适应宽度: {}px", col + 1, max_width as i32).into());
    });
}

/// 设置 hit-col 回调 (精确命中测试, 支持变列宽)
fn setup_hit_col(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    main_window.global::<SpreadsheetModel>().on_hit_col(move |mx| {
        let Some(win) = win_weak.upgrade() else { return -1; };
        let model = win.global::<SpreadsheetModel>();

        let col_count = model.get_col_count();
        if col_count == 0 { return -1; }

        // 读取 col-x 前缀数组进行二分查找
        // col-x[c] = c列左边界, col-x[col-count] = 总宽
        // 找最大 c 满足 col-x[c] <= mx
        let col_x_count = model.get_col_x().row_count();
        if col_x_count > col_count as usize {
            let mut lo = 0i32;
            let mut hi = col_count;  // 独占上界
            while lo < hi {
                let mid = (lo + hi) / 2;
                let mid_x = model.get_col_x().row_data(mid as usize).unwrap_or(0.0f32);
                if mx < mid_x {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            return max(0, lo - 1);
        }

        // 回退: 统一列宽
        max(0, min(col_count - 1, (mx / model.get_col_width()) as i32))
    });
}

// ===== 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    init_spreadsheet(&main_window);
    setup_load_data(&main_window);
    setup_click_handlers(&main_window);
    setup_resize_column(&main_window);
    setup_auto_fit_column(&main_window);
    setup_auto_fit_selection(&main_window);
    setup_hit_col(&main_window);
    setup_copy(&main_window);
    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("Slint Android 初始化失败");

    let main_window = MainWindow::new().expect("创建主窗口失败");
    init_spreadsheet(&main_window);
    setup_load_data(&main_window);
    setup_click_handlers(&main_window);
    setup_resize_column(&main_window);
    setup_auto_fit_column(&main_window);
    setup_auto_fit_selection(&main_window);
    setup_hit_col(&main_window);
    setup_copy(&main_window);
    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
