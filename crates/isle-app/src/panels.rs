use super::*;

const ICE: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 234. / 255.,
    g: 244. / 255.,
    b: 1.,
    a: 1.,
};
const ACCENT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 145. / 255.,
    g: 202. / 255.,
    b: 1.,
    a: 1.,
};
const CAPTION: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 138. / 255.,
    g: 190. / 255.,
    b: 245. / 255.,
    a: 0.58,
};

impl Renderer {
    unsafe fn text_width(
        &mut self,
        text: &str,
        size: u32,
        weight: DWRITE_FONT_WEIGHT,
    ) -> Result<f32> {
        let f = self.format_with_weight(size, weight)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = self.write.CreateTextLayout(&wide, &f, 2000., 100.)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        Ok(metrics.width)
    }
    // RollingNumber.svelte uses fixed .62em digit cells, a .24em colon and
    // .015em gaps. Plain DrawText uses different kerning and cannot match it.
    unsafe fn number(
        &mut self,
        text: &str,
        r: Rect,
        size: u32,
        right: bool,
        ink: D2D1_COLOR_F,
    ) -> Result<()> {
        let em = size as f32;
        let width = text
            .chars()
            .map(|c| if c == ':' { 0.24 } else { 0.62 })
            .sum::<f32>()
            * em
            + text.chars().count().saturating_sub(1) as f32 * 0.015 * em;
        let mut x = if right { r.x + r.w - width } else { r.x };
        let f = self.format_with_weight(size, DWRITE_FONT_WEIGHT_BOLD)?;
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        for character in text.chars() {
            let w = if character == ':' {
                0.24 * em
            } else {
                0.62 * em
            };
            let mut buffer = [0u16; 2];
            self.text_utf16(
                character.encode_utf16(&mut buffer),
                Rect {
                    x,
                    y: r.y,
                    w,
                    h: em,
                },
                size,
                DWRITE_FONT_WEIGHT_BOLD,
                ink,
            )?;
            x += w + 0.015 * em;
        }
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        Ok(())
    }
    unsafe fn label(
        &mut self,
        text: &str,
        r: Rect,
        size: u32,
        weight: DWRITE_FONT_WEIGHT,
        alignment: DWRITE_TEXT_ALIGNMENT,
        ink: D2D1_COLOR_F,
    ) -> Result<()> {
        let f = self.format_with_weight(size, weight)?;
        f.SetTextAlignment(alignment)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let result = self.text_with_weight(text, r, size, weight, ink);
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        result
    }
    unsafe fn ellipsis_label(
        &mut self,
        text: &str,
        r: Rect,
        size: u32,
        ink: D2D1_COLOR_F,
    ) -> Result<()> {
        let f = self.format_with_weight(size, DWRITE_FONT_WEIGHT_BOLD)?;
        let sign = self.write.CreateEllipsisTrimmingSign(&f)?;
        f.SetTrimming(
            &DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                ..Default::default()
            },
            &sign,
        )?;
        let result = self.label(
            text,
            r,
            size,
            DWRITE_FONT_WEIGHT_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            ink,
        );
        f.SetTrimming(&DWRITE_TRIMMING::default(), None)?;
        result
    }
    unsafe fn border(&self, r: Rect, radius: f32, ink: D2D1_COLOR_F) {
        self.ink(ink);
        self.ctx.DrawRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(r),
                radiusX: radius,
                radiusY: radius,
            },
            &self.brush,
            1.,
            None,
        );
    }
    unsafe fn ruler_panel(&mut self, m: &Model, timer: bool) -> Result<()> {
        let r = m.ruler();
        let value = if timer {
            m.timer_minutes as i32
        } else {
            m.volume.round() as i32
        };
        // TimerPanel's ruler is authored with its selected mark at 141 px;
        // VolumePanel centers its selected mark in the available width.
        let center = if timer { 141. } else { r.w / 2. };
        self.ctx
            .PushAxisAlignedClip(&rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        for tick in (value - 30).max(if timer { 1 } else { 0 })..=(value + 30).min(if timer {
            1440
        } else {
            100
        }) {
            let x = r.x + center + (tick - value) as f32 * 10.;
            if x < r.x - 5. || x > r.x + r.w + 5. {
                continue;
            }
            let fade = ((x - r.x) / (r.w * 0.1))
                .min((r.x + r.w - x) / (r.w * 0.1))
                .clamp(0., 1.);
            let major = tick % 5 == 0;
            let selected = tick == value;
            let h = if selected {
                24.
            } else if major {
                24. * 0.852
            } else {
                24. * 0.667
            };
            let mut ink = if tick <= value {
                ICE
            } else if major {
                color(105. / 255., 170. / 255., 1., 0.7)
            } else {
                color(130. / 255., 153. / 255., 185. / 255., 0.3)
            };
            ink.a *= fade;
            self.fill(
                Rect {
                    x: x - 1.5,
                    y: r.y + 41. - h,
                    w: 3.,
                    h,
                },
                1.5,
                ink,
            );
            if major {
                let mut ink = if tick <= value {
                    color(185. / 255., 221. / 255., 1., 1.)
                } else {
                    color(110. / 255., 178. / 255., 1., 0.78)
                };
                ink.a *= fade;
                self.label(
                    &tick.to_string(),
                    Rect {
                        x: x - 18.,
                        y: r.y,
                        w: 36.,
                        h: 15.,
                    },
                    8,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    ink,
                )?;
            }
        }
        self.ctx.PopAxisAlignedClip();
        Ok(())
    }
    pub(super) unsafe fn volume_panel(&mut self, m: &Model, hover: Option<Hit>) -> Result<()> {
        let p = m.detail_content();
        self.ruler_panel(m, false)?;
        let r = m.device_trigger();
        self.fill(
            r,
            8.,
            color(
                1.,
                1.,
                1.,
                if m.device_menu || hover == Some(Hit::Devices) {
                    0.1
                } else {
                    0.07
                },
            ),
        );
        self.border(
            r,
            8.,
            if m.device_menu {
                color(105. / 255., 170. / 255., 1., 0.32)
            } else {
                color(1., 1., 1., 0.1)
            },
        );
        let name = m
            .audio
            .as_ref()
            .map(|a| {
                if a.device.id.is_empty() {
                    "未找到可用设备"
                } else {
                    a.device.name.as_str()
                }
            })
            .unwrap_or("Speakers");
        self.ellipsis_label(
            name,
            Rect {
                x: r.x + 9.,
                y: r.y,
                w: r.w - 35.,
                h: r.h,
            },
            9,
            color(1., 1., 1., 0.82),
        )?;
        self.icons.draw(
            if m.device_menu {
                Icon::ChevronUp
            } else {
                Icon::ChevronDown
            },
            r.x + r.w - 19.,
            r.y + 7.,
            12.,
            2.2,
            color(1., 1., 1., 0.48),
        )?;
        self.number(
            &format!("{}", m.volume.round() as u32),
            Rect {
                x: p.x + p.w - 54.,
                y: p.y + 49.,
                w: 43.64,
                h: 26.,
            },
            24,
            true,
            ICE,
        )?;
        self.label(
            "%",
            Rect {
                x: p.x + p.w - 10.,
                y: p.y + 58.,
                w: 10.,
                h: 12.,
            },
            10,
            DWRITE_FONT_WEIGHT_BOLD,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            CAPTION,
        )?;
        self.label(
            if m.volume == 0. {
                "静音"
            } else {
                "系统音量"
            },
            Rect {
                x: p.x + p.w - 54.,
                y: p.y + 77.,
                w: 54.,
                h: 8.,
            },
            8,
            DWRITE_FONT_WEIGHT_MEDIUM,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            CAPTION,
        )?;
        if m.device_menu {
            let popup = m.device_popup();
            self.fill(popup, 10., color(23. / 255., 23. / 255., 25. / 255., 1.));
            self.border(popup, 10., color(1., 1., 1., 0.12));
            if let Some(audio) = &m.audio {
                let viewport = m.device_viewport();
                self.ctx
                    .PushAxisAlignedClip(&rect(viewport), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                for (hit, _) in m.controls() {
                    if let Hit::Device(index) = hit {
                        let r = m.device_row(index);
                        let device = &audio.devices[index];
                        let selected = device.id == audio.device.id;
                        if selected || hover == Some(hit) || m.focus == Some(hit) {
                            self.fill(
                                r,
                                7.,
                                if selected {
                                    color(65. / 255., 145. / 255., 235. / 255., 0.14)
                                } else {
                                    color(1., 1., 1., 0.08)
                                },
                            );
                        }
                        self.ellipsis_label(
                            &device.name,
                            Rect {
                                x: r.x + 7.,
                                y: r.y,
                                w: r.w - 32.,
                                h: r.h,
                            },
                            9,
                            color(1., 1., 1., if selected { 1. } else { 0.68 }),
                        )?;
                        if selected {
                            self.icons.draw(
                                Icon::Check,
                                r.x + r.w - 19.,
                                r.y + 8.,
                                12.,
                                2.4,
                                ACCENT,
                            )?;
                        }
                    }
                }
                self.ctx.PopAxisAlignedClip();
                if audio.devices.len() > 2 {
                    let content = audio.devices.len() as f32 * 28.;
                    let height = viewport.h * viewport.h / content;
                    let offset = (viewport.h - height) * m.device_scroll / (content - viewport.h);
                    self.fill(
                        Rect {
                            x: popup.x + popup.w - 4.,
                            y: viewport.y + offset,
                            w: 3.,
                            h: height,
                        },
                        1.5,
                        color(1., 1., 1., 0.2),
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) unsafe fn timer_panel(&mut self, m: &Model, hover: Option<Hit>) -> Result<()> {
        let p = m.detail_content();
        if m.timer_finished {
            let r = Rect {
                x: p.x,
                y: p.y + (p.h - 70.) / 2.,
                w: p.w,
                h: 70.,
            };
            self.fill(r, 13., color(1., 1., 1., 0.03));
            self.border(r, 13., color(1., 1., 1., 0.085));
            self.fill(
                Rect {
                    x: r.x + 11.,
                    y: r.y + 20.,
                    w: 30.,
                    h: 30.,
                },
                15.,
                color(67. / 255., 155. / 255., 1., 0.14),
            );
            self.icons
                .draw(Icon::Check, r.x + 17.5, r.y + 26.5, 17., 2.4, ACCENT)?;
            self.label(
                "计时完成",
                Rect {
                    x: r.x + 54.,
                    y: r.y + 18.,
                    w: r.w - 98.,
                    h: 17.,
                },
                13,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                ICE,
            )?;
            self.label(
                "倒计时已结束",
                Rect {
                    x: r.x + 54.,
                    y: r.y + 40.,
                    w: r.w - 98.,
                    h: 13.,
                },
                10,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                color(229. / 255., 235. / 255., 245. / 255., 0.52),
            )?;
        } else {
            if !m.timer_active {
                self.ruler_panel(m, true)?;
            }
            let y = p.y
                + if m.timer_active {
                    (p.h - 37.6) / 2.
                } else {
                    49.
                };
            self.number(
                &countdown_label(m.timer_left),
                Rect {
                    x: p.x + 110.,
                    y,
                    w: p.w - 116.,
                    h: 26.,
                },
                24,
                true,
                ICE,
            )?;
            self.label(
                if !m.timer_active {
                    "准备开始"
                } else if m.timer_deadline.is_some() {
                    "进行中"
                } else {
                    "已暂停"
                },
                Rect {
                    x: p.x + 110.,
                    y: y + 29.6,
                    w: p.w - 110.,
                    h: 10.,
                },
                8,
                DWRITE_FONT_WEIGHT_MEDIUM,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                CAPTION,
            )?;
        }
        for (hit, r) in m.controls() {
            if !matches!(hit, Hit::Timer | Hit::Reset | Hit::Dismiss) {
                continue;
            }
            if hit == Hit::Timer && !m.timer_active {
                self.fill(
                    r,
                    17.,
                    color(
                        65. / 255.,
                        145. / 255.,
                        235. / 255.,
                        if hover == Some(hit) { 0.22 } else { 0.14 },
                    ),
                );
                self.label(
                    "开始倒计时",
                    r,
                    10,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    ACCENT,
                )?;
            } else {
                if hover == Some(hit) || m.focus == Some(hit) {
                    self.fill(
                        r,
                        if hit == Hit::Dismiss { 13.5 } else { 12. },
                        color(1., 1., 1., 0.1),
                    );
                }
                let icon = if hit == Hit::Timer {
                    if m.timer_deadline.is_some() {
                        Icon::Pause
                    } else {
                        Icon::Play
                    }
                } else {
                    Icon::Close
                };
                let size = if hit == Hit::Dismiss { 14. } else { 20. };
                self.icons.draw(
                    icon,
                    r.x + (r.w - size) / 2.,
                    r.y + (r.h - size) / 2.,
                    size,
                    if hit == Hit::Timer { 2. } else { 2.2 },
                    color(1., 1., 1., 0.9),
                )?;
            }
        }
        Ok(())
    }
    pub(super) unsafe fn clock_panel(&mut self, m: &Model) -> Result<()> {
        let p = m.detail_content();
        let time = crate::clock::now(&m.time_zone);
        let st = time.unwrap_or_default();
        let date = if time.is_some() {
            format!(
                "{}年{}月{}日周{}",
                st.wYear,
                st.wMonth,
                st.wDay,
                ["日", "一", "二", "三", "四", "五", "六"][st.wDayOfWeek.min(6) as usize]
            )
        } else {
            "无法读取所选时区".into()
        };
        let y = p.y + (p.h - 62.8) / 2.;
        self.label(
            &date,
            Rect {
                x: p.x,
                y,
                w: p.w,
                h: 11.,
            },
            9,
            DWRITE_FONT_WEIGHT_MEDIUM,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            color(1., 1., 1., 0.5),
        )?;
        self.number(
            &if time.is_some() {
                format!("{:02}:{:02}", st.wHour, st.wMinute)
            } else {
                "--:--".into()
            },
            Rect {
                x: p.x,
                y: y + 10.8,
                w: p.w,
                h: 54.,
            },
            52,
            false,
            color(1., 1., 1., 1.),
        )
    }
    pub(super) unsafe fn weather_panel(&mut self, m: &Model, hover: Option<Hit>) -> Result<()> {
        let p = m.detail_content();
        if let Some(city) = &m.weather.city {
            let data = m.weather.data.as_ref();
            let status = if m.weather.failed {
                Some("天气暂时无法更新")
            } else if data.is_none() {
                Some("正在更新天气…")
            } else {
                None
            };
            let y = p.y + (p.h - if status.is_some() { 193. } else { 166. }) / 2.;
            let city_width = self
                .text_width(&city.name, 12, DWRITE_FONT_WEIGHT_BOLD)?
                .min(p.w - 126.);
            self.ellipsis_label(
                &city.name,
                Rect {
                    x: p.x,
                    y: y + 20.,
                    w: city_width,
                    h: 18.,
                },
                12,
                color(243. / 255., 246. / 255., 250. / 255., 1.),
            )?;
            self.label(
                &data.map_or_else(|| "--°".into(), |d| format!("{:.0}°", d.temperature)),
                Rect {
                    x: p.x + city_width + 12.,
                    y,
                    w: 66.,
                    h: 45.,
                },
                30,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                ICE,
            )?;
            self.icons.draw(
                crate::icons::weather(data.map(|d| d.code)),
                p.x + p.w - 36.,
                y + 4.5,
                36.,
                1.8,
                ICE,
            )?;
            let forecast_y = y + 57. + if status.is_some() { 27. } else { 0. };
            if let Some(status) = status {
                self.label(
                    status,
                    Rect {
                        x: p.x,
                        y: y + 57.,
                        w: p.w,
                        h: 15.,
                    },
                    10,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    color(1., 1., 1., 0.5),
                )?;
            }
            if let Some(data) = data {
                for (i, day) in data.days.iter().take(3).enumerate() {
                    let width = self
                        .text_width(
                            day.date.get(5..).unwrap_or("--"),
                            11,
                            DWRITE_FONT_WEIGHT_NORMAL,
                        )?
                        .max(19.);
                    let x = p.x + i as f32 * (p.w - width) / 2.;
                    self.label(
                        day.date.get(5..).unwrap_or("--"),
                        Rect {
                            x,
                            y: forecast_y,
                            w: width,
                            h: 16.5,
                        },
                        11,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        ICE,
                    )?;
                    self.icons.draw(
                        crate::icons::weather(Some(day.code)),
                        x + (width - 19.) / 2.,
                        forecast_y + 21.5,
                        19.,
                        1.8,
                        ICE,
                    )?;
                    self.label(
                        &format!("{:.0}°", day.high),
                        Rect {
                            x,
                            y: forecast_y + 45.5,
                            w: width,
                            h: 16.5,
                        },
                        11,
                        DWRITE_FONT_WEIGHT_BOLD,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        ICE,
                    )?;
                    self.label(
                        &format!("{:.0}°", day.low),
                        Rect {
                            x,
                            y: forecast_y + 67.,
                            w: width,
                            h: 15.,
                        },
                        10,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        color(1., 1., 1., 0.5),
                    )?;
                }
                self.label(
                    &format!("更新于 {}", data.observed.get(11..).unwrap_or("--:--")),
                    Rect {
                        x: p.x,
                        y: forecast_y + 94.,
                        w: p.w,
                        h: 15.,
                    },
                    10,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    color(1., 1., 1., 0.5),
                )?;
            }
        } else {
            self.label(
                "请先设置天气城市",
                Rect {
                    x: p.x,
                    y: p.y + p.h / 2. - 31.,
                    w: p.w,
                    h: 15.,
                },
                10,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                color(1., 1., 1., 0.5),
            )?;
            for (hit, r) in m.controls() {
                if hit == Hit::WeatherSettings {
                    if hover == Some(hit) || m.focus == Some(hit) {
                        self.fill(r, 12., color(1., 1., 1., 0.1));
                    }
                    self.label(
                        "打开设置",
                        r,
                        14,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        color(1., 1., 1., 1.),
                    )?;
                }
            }
        }
        Ok(())
    }
    pub(super) unsafe fn compact_timer(&mut self, m: &Model) -> Result<()> {
        let o = m.origin();
        let vertical = vertical(m.edge);
        let ratio = if m.timer_finished {
            1.
        } else {
            (m.timer_left / m.timer_duration.max(1.)).clamp(0., 1.) as f32
        };
        let progress = if m.timer_finished {
            color(66. / 255., 217. / 255., 154. / 255., 1.)
        } else if m.timer_deadline.is_none() {
            color(109. / 255., 143. / 255., 185. / 255., 1.)
        } else {
            color(38. / 255., 136. / 255., 1., 1.)
        };
        let track = if vertical {
            Rect {
                x: o.x + 5.,
                y: o.y + 14.,
                w: 2.,
                h: m.height.value - 28.,
            }
        } else {
            Rect {
                x: o.x + 14.,
                y: o.y + m.height.value - 7.,
                w: m.width.value - 28.,
                h: 2.,
            }
        };
        self.fill(track, 1., color(115. / 255., 143. / 255., 183. / 255., 0.2));
        self.fill(
            Rect {
                w: if vertical { track.w } else { track.w * ratio },
                h: if vertical { track.h * ratio } else { track.h },
                ..track
            },
            1.,
            progress,
        );
        let (x, y) = if vertical {
            (
                o.x + (m.width.value - 14.) / 2.,
                o.y + m.height.value / 2. - 24.,
            )
        } else {
            (o.x + 14., o.y + (m.height.value - 18.) / 2.)
        };
        self.icons.draw(
            Icon::Timer,
            x,
            y,
            14.,
            1.8,
            color(112. / 255., 170. / 255., 1., 1.),
        )?;
        if !m.timer_finished && m.timer_deadline.is_none() {
            self.icons.draw(
                Icon::Pause,
                x + if vertical { 3. } else { 21. },
                y + if vertical { 19. } else { 3. },
                8.,
                2.,
                color(168. / 255., 197. / 255., 233. / 255., 1.),
            )?;
        }
        let r = if vertical {
            Rect {
                x: o.x + 4.,
                y: o.y + m.height.value / 2. + 10.,
                w: m.width.value - 8.,
                h: 13.,
            }
        } else {
            Rect {
                x: o.x + 50.,
                y: o.y + (m.height.value - 17.) / 2.,
                w: m.width.value - 64.,
                h: 13.,
            }
        };
        self.ctx
            .PushAxisAlignedClip(&rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        if m.timer_finished {
            self.label(
                "计时完成",
                r,
                11,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                ICE,
            )?;
        } else {
            self.number(
                &countdown_label(m.timer_left),
                r,
                if vertical { 11 } else { 13 },
                true,
                ICE,
            )?;
        }
        self.ctx.PopAxisAlignedClip();
        Ok(())
    }
}

fn countdown_label(seconds: f64) -> String {
    let total = seconds.max(0.).ceil() as u64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total / 60) % 60,
            total % 60
        )
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}
