use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use regex::Regex;

use crate::model::DanmakuMessageChat;

// 转换时间的函数
pub fn sec2hms(sec: f64) -> String {
    let hours = (sec / 3600.0).floor() as i32;
    let minutes = ((sec % 3600.0) / 60.0).floor() as i32;
    let seconds = ((sec % 60.0) * 100f64).round() / 100f64;
    format!("{hours:02}:{minutes:02}:{seconds}")
}

pub fn xml2ass(xml_name: &str) -> io::Result<()> {
    let path = Path::new(xml_name);
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let mut chats: Vec<DanmakuMessageChat> = serde_json::from_str(&contents)?;

    chats.sort_by_key(|chat| chat.vpos.unwrap_or(0));

    // 获取运营弹幕ID和需要过滤弹幕的ID
    let mut office_ids = Vec::new();
    let mut filtered_chats = Vec::new();
    for chat in &chats {
        // if chat.content.is_none() {
        //     continue;
        // }
        let user_id = &chat.user_id;
        let premium = chat.premium;
        if matches!(premium, Some(3)) || matches!(premium, Some(7)) || matches!(premium, Some(-1)) {
            office_ids.push(user_id.clone());
        }
        filtered_chats.push(chat);
    }

    if office_ids.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "找不到运营id，请手动输入",
        ));
    }

    // 弹幕参数
    let aa_size = 18; // AA弹幕字体大小
    let aa_high_adjust = 0; // AA弹幕行间间隔
    let office_size = 40; // 运营弹幕字体大小
    let office_bg_height = 72; // 运营弹幕背景遮盖高度
    let font_name = "SourceHanSansJP-Bold"; // 弹幕字体
    let danmaku_size = 68; // 弹幕字体大小
    let danmaku_line_height = 64; // 弹幕行高度
    let danmaku_font_space = 2; // 弹幕行间间隔
    let time_danmaku = 8i64; // 普通弹幕持续时间，默认8秒
    let limit_line_amount = 11; // 屏上弹幕行数限制
    let mut danmaku_passageway = vec![0; limit_line_amount]; // 计算弹幕应该在哪一行出现
    let mut dm_count = 0; // 处理同时出过多弹幕的情况
    let mut vpos_now = 0;

    let mut vote_check = false; // 判断投票是否开启
    let color_map: HashMap<_, _> = vec![
        ("black", "000000"),
        ("white", "FFFFFF"),
        ("red", "FF0000"),
        ("green", "00ff00"),
        ("yellow", "FFFF00"),
        ("blue", "0000FF"),
        ("orange", "ffcc00"),
        ("pink", "FF8080"),
        ("cyan", "00FFFF"),
        ("purple", "C000FF"),
        ("niconicowhite", "cccc99"),
        ("white2", "cccc99"),
        ("truered", "cc0033"),
        ("red2", "cc0033"),
        ("passionorange", "ff6600"),
        ("orange2", "ff6600"),
        ("madyellow", "999900"),
        ("yellow2", "999900"),
        ("elementalgreen", "00cc66"),
        ("green2", "00cc66"),
        ("marineblue", "33ffcc"),
        ("blue2", "33ffcc"),
        ("nobleviolet", "6633cc"),
        ("purple2", "6633cc"),
    ]
    .into_iter()
    .collect(); // 颜色列表
    let video_width = 1280; // 视频宽度，默认3M码率生放，不用改
    let video_height = 720; // 视频高度，默认3M码率生放，不用改
    let font_size = 64; // 普通弹幕字体大小

    let mut aa_events = vec!["Comment: 0,0:00:00.00,0:00:00.00,AA,,0,0,0,,AA弹幕".to_string()]; // eventA
    let mut office_events =
        vec!["Comment: 0,0:00:00.00,0:00:00.00,Office,,0,0,0,,运营弹幕".to_string()]; // eventO
    let mut danmaku_events =
        vec!["Comment: 0,0:00:00.00,0:00:00.00,Danmaku,,0,0,0,,普通弹幕".to_string()]; // eventD
    let office_bg = format!(
        "m 0 0 l {video_width} 0 l {video_width} {office_bg_height} l 0 {office_bg_height}"
    ); // 运营弹幕遮盖

    let mut include_aa = false; // 判断是否有AA弹幕

    let mut official_check = false;
    let mut start_time_w = "".to_string();
    let mut end_time_w = "".to_string();
    let mut text_w = "".to_string();
    let mut vpos_w = 0;
    let mut ass_color = String::new();

    let mut start_time_q = String::new();
    let mut start_time_r = String::new();
    let mut text_q = String::new();
    let mut text_o = Vec::new();
    let mut text_r = Vec::new();

    for chat in filtered_chats.iter() {
        let ref text = chat.content;
        let ref user_id = chat.user_id;
        let mail = chat.mail.as_deref().unwrap_or("");
        let premium = chat.premium;
        let Some(vpos) = chat.vpos else {
            continue;
        };
        // FIXME: round
        let start_time = sec2hms((vpos as f64) / 100.0);
        let end_time = sec2hms((vpos as f64) / 100.0 + time_danmaku as f64);
        let mut color = "ffffff".to_string();
        let mut color_important = None;

        let mut passageway_index = 0;
        let mut passageway_min = 0;

        // 过滤弹幕
        let ng_words = [
            "※ NGコメント",
            "/clear",
            "/trialpanel",
            "/spi",
            "/disconnect",
            "/gift",
            "/commentlock",
            "/nicoad",
            "/info",
            "/jump",
            "/play",
            "/redirect",
        ];
        if ng_words.iter().any(|ngword| text.contains(ngword)) {
            continue;
        }
        if let Some(2) = premium {
            continue;
        }

        // 释放之前捕捉的运营弹幕
        if official_check {
            if vpos - vpos_w > 800 || office_ids.contains(&user_id) {
                if office_ids.contains(&user_id) {
                    end_time_w = start_time.clone();
                }
                let event_bg = format!(
                    "Dialogue: 4,{start_time_w},{end_time_w},Office,,0,0,0,,{{\\an5\\p1\\pos({},{})\\bord0\\1c&H000000&\\1a&H78&}}{office_bg}",
                    video_width / 2,
                    office_bg_height / 2,
                );
                let mut event_dm = if text_w.contains("href") {
                    let link = Regex::new(r#"<a href=(.*?)><u>"#).unwrap();
                    // TODO: maybe not correct
                    let text_w = link.replace_all(&text_w, "").replace("</u></a>", "");
                    format!(
                        "Dialogue: 5,{start_time_w},{end_time_w},Office,,0,0,0,,{{\\an5\\pos({},{})\\bord0\\1c&HFF8000&\\u1\\fsp0}}{}",
                        video_width / 2,
                        office_bg_height / 2,
                        text_w.replace("/perm ", "")
                    )
                } else {
                    format!(
                        "Dialogue: 5,{start_time_w},{end_time_w},Office,,0,0,0,,{{\\an5\\pos({},{})\\bord0{ass_color}\\fsp0}}{}",
                        video_width / 2,
                        office_bg_height / 2,
                        text_w.replace("/perm ", "")
                    )
                };
                if text.chars().count() > 50 {
                    event_dm = event_dm.replace("fsp0", "fsp0\\fs30");
                }
                office_events.push(event_bg);
                office_events.push(event_dm.replace("　", "  "));
                official_check = false;
            }
        }

        // 颜色调整
        for style in mail.split_whitespace() {
            let re = Regex::new(r"#([0-9A-Fa-f]{6})").unwrap();
            if let Some(m) = re.captures(style) {
                color_important = m.get(1).map(|m| m.as_str());
            } else if let Some(color_got) = color_map.get(style) {
                color = color_got.to_string();
            }

            if let Some(color_important) = color_important {
                color = color_important.to_string();
            }
            ass_color = format!("\\1c&H{}{}{}&", &color[4..6], &color[2..4], &color[0..2]);
            if color == "000000" {
                ass_color += "\\3c&HFFFFFF&";
            }
        }

        // 处理运营弹幕
        if office_ids.contains(user_id) {
            // 处理投票开始和投票结果
            if text.starts_with("/vote") && !text.starts_with("/vote stop") {
                let split_text: Vec<_> = shlex::split(text)
                    .unwrap()
                    .into_iter()
                    .map(|text| text.replace('\\', ""))
                    .collect();
                if split_text[1] == "start" {
                    start_time_q = start_time;
                    text_q = split_text[2].to_string();
                    text_o = split_text[3..].to_vec();
                    text_r = Vec::new();
                    vote_check = true;
                } else if split_text[1] == "showresult" {
                    start_time_r = start_time;
                    text_r = split_text[3..].to_vec();
                }
                continue;
            } else if vote_check {
                // 生成投票
                let end_time_v = sec2hms((vpos as f64) / 100.0);
                let event_q_bg = format!(
                    "Dialogue: 4,{start_time_q},{end_time_v},Office,,0,0,0,,{{\\an5\\p1\\pos({},{})\\bord0\\1c&H000000&\\1a&H78&}}{}",
                    video_width / 2,
                    office_bg_height / 2,
                    office_bg
                );
                let mut event_q_text = format!(
                    "Dialogue: 5,{start_time_q},{end_time_v},Office,,0,0,0,,{{\\an5\\pos({},{})\\1c&HFF8000&\\bord0\\fsp0}}Q.{{\\1c&HFFFFFF&}}{}",
                    video_width / 2,
                    office_bg_height / 2,
                    text_q.replace("<br>", "\\N")
                );
                let event_q_mask = format!(
                    "Dialogue: 3,{start_time_q},{end_time_v},Office,,0,0,0,,{{\\an5\\p1\\bord0\\1c&H000000&\\pos({},{})\\1a&HC8&}}m 0 0 l {} 0 l {} {} l 0 {}",
                    video_width / 2,
                    video_height / 2,
                    video_width + 20,
                    video_width + 20,
                    video_height + 20,
                    video_height + 20,
                );
                if text_q.chars().count() > 50 {
                    event_q_text = event_q_text.replace("fsp0", "fsp0\\fs30");
                }
                office_events.push(event_q_bg);
                office_events.push(event_q_text);
                office_events.push(event_q_mask);

                let font_size_anketo = (font_size / 4) * 3;
                if text_o.len() <= 3 {
                    let bg_width = video_width / 4;
                    let bg_height = video_height / 3;
                    let x_array = vec![
                        vec![bg_width / 2],
                        vec![
                            video_width / 3 - 40,
                            (video_width / 2 - video_width / 3) + video_width / 2 + 40,
                        ],
                        vec![
                            video_width / 2 - bg_width - 40,
                            video_width / 2,
                            video_width / 2 + bg_width + 40,
                        ],
                    ];
                    let num_bg = format!(
                        "m 0 0 l {} 0 l {} 0 l 0 {}",
                        font_size * 3 / 2,
                        font_size * 3 / 2,
                        font_size * 3 / 2
                    );
                    let bg = format!(
                        "m 0 0 l {} 0 l {} {} l 0 {}",
                        bg_width, bg_width, bg_height, bg_height
                    );
                    let result_bg = "m 0 0 s 150 0 150 60 0 60 c";
                    let x = &x_array[text_o.len() - 1];
                    let y = vec![360];
                    for j in 0..y.len() {
                        for i in 0..x.len() {
                            let vote_num_bg = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\bord0\\1c&HFFFFC8&\\pos({},{})}}{}",
                                x[i] - bg_width / 2 + font_size  * 5 / 8,
                                y[j] - bg_height / 2 + font_size  * 5 / 8,
                                num_bg
                            );
                            let vote_num_text = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\bord0\\1c&HD5A07B&\\pos({},{})}}{}",
                                x[i] - bg_width / 2 + font_size / 2,
                                y[j] - bg_height / 2 + font_size / 2,
                                i + 1
                            );
                            let vote_bg = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\3c&HFFFFC8&\\bord6\\1c&HD5A07B&\\1a&H78&\\pos({},{})}}{}",
                                x[i],
                                y[j],
                                bg
                            );
                            let text_o_chars = text_o[i].chars().collect::<Vec<_>>();
                            let text_now = if text_o_chars.len() <= 7 {
                                format!("\\N{}", text_o[i])
                            } else if text_o_chars.len() > 7 && text_o_chars.len() <= 14 {
                                format!(
                                    "\\N{}\\N{}",
                                    text_o_chars[0..7].iter().collect::<String>(),
                                    text_o_chars[7..].iter().collect::<String>()
                                )
                            } else {
                                format!(
                                    "\\N{}\\N{}\\N{}",
                                    text_o_chars[0..7].iter().collect::<String>(),
                                    text_o_chars[7..14].iter().collect::<String>(),
                                    text_o_chars[14..].iter().collect::<String>()
                                )
                            };
                            let vote_text = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\bord0\\1c&HFFFFFF\\pos({},{})}}{}",
                                x[i], y[j], text_now
                            );
                            office_events.push(vote_bg);
                            office_events.push(vote_text);
                            office_events.push(vote_num_bg);
                            office_events.push(vote_num_text);

                            if !text_r.is_empty() {
                                let vote_result_bg = format!(
                                    "Dialogue: 5,{start_time_r},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\bord0\\1c&H3E2E2A&\\pos({},{})}}{}",
                                    x[i],
                                    y[j] + bg_height / 2,
                                    result_bg
                                );
                                let vote_result_text = format!(
                                    "Dialogue: 5,{start_time_r},{end_time_v},Anketo,,0,0,0,,{{\\fs{}\\an5\\bord0\\1c&H76FAF8&\\pos({},{})}}{}%",
                                    font_size_anketo,
                                    x[i],
                                    y[j] + bg_height / 2,
                                    text_r[i].parse::<f64>().unwrap() / 10f64
                                );
                                office_events.push(vote_result_bg);
                                office_events.push(vote_result_text);
                            }
                        }
                    }
                } else if text_o.len() >= 4 {
                    let mut bg_width = video_width / 5;
                    let mut bg_height = video_height / 4;
                    let x_array = vec![
                        vec![bg_width / 2],
                        vec![
                            video_width / 3 - 40,
                            (video_width / 2 - video_width / 3) + video_width / 2 + 40,
                        ],
                        vec![
                            video_width / 2 - bg_width - 40,
                            video_width / 2,
                            video_width / 2 + bg_width + 40,
                        ],
                    ];
                    let y_array = vec![
                        vec![video_height / 2],
                        vec![
                            video_height / 3,
                            (video_height / 2 - video_height / 3) + video_height / 2,
                        ],
                        vec![
                            video_height / 2 - bg_height - 20,
                            video_height / 2 + 20,
                            video_height / 2 + bg_height + 60,
                        ],
                    ];
                    let mut x = x_array[2].clone();
                    let mut y = &y_array[2];
                    if text_o.len() == 4 {
                        bg_width = video_width / 4;
                        bg_height = video_height / 4;
                        x = x_array[1].clone();
                        y = &y_array[1];
                    } else if text_o.len() >= 5 && text_o.len() <= 6 {
                        bg_height = video_height / 4;
                        y = &y_array[1];
                    } else if text_o.len() == 8 {
                        x = vec![160, 480, 800, 1120];
                        y = &y_array[1];
                    } else if text_o.len() > 6 {
                        bg_height = video_height * 9 / 2;
                        y = &y_array[2];
                    }

                    let num_bg = format!(
                        "m 0 0 l {} 0 l {} 0 l 0 {}",
                        font_size_anketo * 5 / 4,
                        font_size_anketo * 5 / 4,
                        font_size_anketo * 5 / 4
                    );
                    let bg =
                        format!("m 0 0 l {bg_width} 0 l {bg_width} {bg_height} l 0 {bg_height}");
                    let result_bg = "m 0 0 s 150 0 150 60 0 60 c";

                    let mut num = 0;
                    for j in 0..y.len() {
                        for i in 0..x.len() {
                            if num == text_o.len() {
                                continue;
                            }
                            let vote_num_bg = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\bord0\\1c&HFFFFC8&\\pos({},{})}}{}",
                                x[i] - bg_width / 2 + font_size_anketo * 5 / 8,
                                y[j] - bg_height / 2 + font_size_anketo * 5 / 8,
                                num_bg
                            );
                            let vote_num_text = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\bord0\\1c&HD5A07B&\\pos({},{})}}{}",
                                x[i] - bg_width / 2 + font_size_anketo / 2,
                                y[j] - bg_height / 2 + font_size_anketo / 2,
                                num + 1
                            );
                            let vote_bg = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\3c&HFFFFC8&\\bord6\\1c&HD5A07B&\\1a&H78&\\pos({},{})}}{}",
                                x[i],
                                y[j],
                                bg
                            );
                            let text_o_chars = text_o[num].chars().collect::<Vec<_>>();
                            let text_now = if text_o_chars.len() <= 7 {
                                text_o[num].to_string()
                            } else if text_o_chars.len() > 7 && text_o_chars.len() <= 14 {
                                format!(
                                    "{}\\N{}",
                                    text_o_chars[0..7].iter().collect::<String>(),
                                    text_o_chars[7..].iter().collect::<String>()
                                )
                            } else {
                                format!(
                                    "{}\\N{}\\N{}",
                                    text_o_chars[0..7].iter().collect::<String>(),
                                    text_o_chars[7..14].iter().collect::<String>(),
                                    text_o_chars[14..].iter().collect::<String>()
                                )
                            };
                            let vote_text = format!(
                                "Dialogue: 5,{start_time_q},{end_time_v},Anketo,,0,0,0,,{{\\fs{}\\an5\\bord0\\1c&HFFFFFF&\\pos({},{})}}{}",
                                font_size_anketo,
                                x[i],
                                y[j],
                                text_now
                            );
                            office_events.push(vote_bg);
                            office_events.push(vote_text);
                            office_events.push(vote_num_bg);
                            office_events.push(vote_num_text);

                            if !text_r.is_empty() {
                                let vote_result_bg = format!(
                                    "Dialogue: 5,{start_time_r},{end_time_v},Anketo,,0,0,0,,{{\\an5\\p1\\bord0\\1c&H3E2E2A&\\pos({},{})}}{}",
                                    x[i],
                                    y[j] + bg_height / 2,
                                    result_bg
                                );
                                let vote_result_text = format!(
                                    "Dialogue: 5,{start_time_r},{end_time_v},Anketo,,0,0,0,,{{\\fs{}\\an5\\bord0\\1c&H76FAF8&\\pos({},{})}}{}%",
                                    font_size_anketo,
                                    x[i],
                                    y[j] + bg_height / 2,
                                    text_r[num].parse::<f64>().unwrap() / 10f64
                                );
                                office_events.push(vote_result_bg);
                                office_events.push(vote_result_text);
                            }
                            num += 1;
                        }
                    }
                }
                vote_check = false;
            }

            if !text.contains("/vote") {
                // 处理非投票运营弹幕
                start_time_w = start_time.clone();
                end_time_w = end_time.clone();
                text_w = text.clone();
                vpos_w = vpos;
                official_check = true;
            }
        } else {
            // 处理用户弹幕
            let mut pos = 0;
            let mut is_aa = false;
            let text = text.replace('\n', "\\N");
            for style in mail.split(' ') {
                if style == "ue" {
                    pos = 8;
                } else if style == "shita" {
                    pos = 2;
                } else if style == "gothic" || style == "mincho" {
                    is_aa = true;
                    include_aa = true;
                }
            }
            if is_aa {
                // AA弹幕跳过，在后一部分处理
                continue;
            } else if pos == 2 || pos == 8 {
                // 底部弹幕 / 顶部弹幕
                danmaku_events.push(format!("Dialogue: 2,{start_time},{end_time},Danmaku,,0,0,0,,{{\\an{pos}{ass_color}}}{text}"));
            } else if pos == 0 {
                // 普通滚动弹幕
                if vpos > vpos_now {
                    vpos_now = vpos;
                    dm_count = 0;
                }
                let mut vpos_next_min = i64::MAX;
                let vpos_next =
                    vpos + 1280 / (text.chars().count() as i64 * 70 + 1280) * time_danmaku * 100; // 弹幕不是太密集时，控制同一条通道的弹幕不超过前一行
                dm_count += 1;

                for i in 0..limit_line_amount {
                    if vpos_next >= danmaku_passageway[i] {
                        passageway_index = i;
                        danmaku_passageway[i] = vpos + time_danmaku * 100;
                        break;
                    } else if danmaku_passageway[i] < vpos_next_min {
                        vpos_next_min = danmaku_passageway[i];
                        passageway_min = i;
                    }
                    if i == limit_line_amount - 1 && vpos_next < vpos_next_min {
                        passageway_index = passageway_min;
                        danmaku_passageway[passageway_min] = vpos + time_danmaku * 100;
                    }
                }
                if dm_count > 11 {
                    passageway_index = dm_count % 11;
                }
                // 计算弹幕位置
                let sx = video_width;
                let sy = danmaku_line_height * passageway_index;
                let ex = -(text.chars().count() as i64) * (danmaku_size + danmaku_font_space);
                let ey = danmaku_line_height * passageway_index;
                // 生成弹幕行并加入总弹幕
                if matches!(premium, Some(24)) || matches!(premium, Some(25)) {
                    danmaku_events.push(format!("Dialogue: 2,{start_time},{end_time},Danmaku,,0,0,0,,{{\\an7\\alpha80\\move({sx},{sy},{ex},{ey}){ass_color}}}{text}"));
                } else {
                    // no alpha 80
                    danmaku_events.push(format!("Dialogue: 2,{start_time},{end_time},Danmaku,,0,0,0,,{{\\an7\\move({sx},{sy},{ex},{ey}){ass_color}}}{text}"));
                }
            }
        }
    }

    if include_aa {
        for chat in filtered_chats.iter() {
            let mail = chat.mail.as_deref().unwrap_or_default();
            let styles: Vec<_> = mail.split(' ').collect();
            if styles.contains(&"mincho") || styles.contains(&"gothic") {
                let text = &chat.content;
                let vpos = chat.vpos.unwrap();
                let start_time = sec2hms((vpos as f64) / 100.0);
                let end_time = sec2hms((vpos as f64) / 100.0 + time_danmaku as f64);

                let mut color = "ffffff";
                let mut color_important = None;

                for style in styles {
                    let re = Regex::new(r"#([0-9A-Fa-f]{6})").unwrap();
                    if let Some(m) = re.captures(style) {
                        color_important = m.get(1).map(|m| m.as_str());
                    } else if let Some(color_got) = color_map.get(style) {
                        color = color_got;
                    }
                }

                if let Some(color_important) = color_important {
                    color = color_important;
                }

                let mut ass_color =
                    format!("\\1c&H{}{}{}&", &color[4..6], &color[2..4], &color[0..2]);
                if color == "000000" {
                    ass_color += "\\3c&HFFFFFF&";
                }

                // 分成多行生成弹幕并整合成完整AA弹幕
                let text_aa = text.split('\n');
                for (a, content) in text_aa.enumerate() {
                    aa_events.push(format!("Dialogue: 1,{start_time},{end_time},AA,,0,0,0,,{{\\an4\\fsp-1\\move({video_width},{},{},{}){ass_color}}}{content}",(aa_size-1)*a+aa_high_adjust,-font_size*10,(aa_size-1)*a+aa_high_adjust));
                }
            }
        }
    }

    // 写入 .ass 文件
    let ass_file_name = format!("{}.ass", path.file_stem().unwrap().to_str().unwrap());
    let mut ass_file = File::create(&ass_file_name)?;

    writeln!(ass_file, "[Script Info]")?;
    writeln!(ass_file, "; Script generated by Aegisub 3.2.2")?;
    writeln!(ass_file, "; http://www.aegisub.org/")?;
    writeln!(ass_file, "ScriptType: v4.00+")?;
    writeln!(ass_file, "PlayResX: 1280")?;
    writeln!(ass_file, "PlayResY: 720")?;
    writeln!(ass_file)?;
    writeln!(ass_file, "[V4+ Styles]")?;
    writeln!(ass_file, "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, marginL, marginR, marginV, Encoding")?;
    writeln!(ass_file, "Style: Default,微软雅黑,54,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,0,0,0,0")?;
    writeln!(ass_file, "Style: Alternate,微软雅黑,36,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,0,0,0,0")?;
    writeln!(ass_file, "Style: AA,黑体,{aa_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,0,0,2,0,0,0,0")?;
    writeln!(ass_file, "Style: Office,{font_name},{office_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,2,0,1,1.5,0,2,0,0,10,0")?;
    writeln!(ass_file, "Style: Anketo,{font_name},{font_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,2,0,1,1.5,0,2,0,0,10,0")?;
    writeln!(ass_file, "Style: Danmaku,{font_name},{font_size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,2,0,1,1.5,0,2,0,0,10,0")?;
    writeln!(ass_file)?;
    writeln!(ass_file, "[Events]")?;
    writeln!(
        ass_file,
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
    )?;

    for event in office_events {
        writeln!(ass_file, "{}", event)?;
    }
    for event in danmaku_events {
        writeln!(ass_file, "{}", event)?;
    }
    for event in aa_events {
        writeln!(ass_file, "{}", event)?;
    }

    Ok(())
}

#[test]
fn test_generate() {
    let input =
        "/Users/yesterday17/Development/Me/iori/crates/nicolive/test/danmaku/json/548本篇.json";
    xml2ass(input).unwrap();
}
