use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use super::app::{App, Pane};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewLayout {
    pub search: Rect,
    pub categories: Rect,
    pub bookmarks: Rect,
}

impl ViewLayout {
    pub fn category_content(self) -> Rect {
        bordered_inner(self.categories)
    }

    pub fn bookmark_content(self) -> Rect {
        bordered_inner(self.bookmarks)
    }
}

pub(crate) fn layout_for(area: Rect) -> ViewLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(vertical[1]);
    ViewLayout {
        search: vertical[0],
        categories: body[0],
        bookmarks: body[1],
    }
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);
    let layout = layout_for(area);
    let category_rows = usize::from(layout.category_content().height);
    let bookmark_rows = usize::from(layout.bookmark_content().height);
    app.set_viewport_rows(category_rows, bookmark_rows);

    let search_mode = if app.is_searching() {
        " [输入中]"
    } else {
        ""
    };
    let search = if app.search_text().is_empty() {
        "/".to_owned()
    } else {
        app.search_text().to_owned()
    };
    frame.render_widget(
        Paragraph::new(Line::from(format!("mkd  搜索: {search}{search_mode}")))
            .block(Block::default().borders(Borders::ALL).title("状态")),
        vertical[0],
    );

    render_categories(frame, app, layout);
    render_bookmarks(frame, app, layout);

    let footer = match app.status_message() {
        Some(message) => format!("错误: {message}"),
        None if app.is_searching() => {
            "搜索中: 输入文字过滤  Enter 跳转  Esc 退出搜索（管理操作需先退搜索）".to_owned()
        }
        None => "↑/↓ 或 j/k 移动  Tab 切栏  / 搜索  c 新建分组  m 归组  e 改名  y 复制  Enter 跳转"
            .to_owned(),
    };
    let footer_style = if app.status_message().is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(footer).style(footer_style), vertical[2]);

    if app.is_confirming_delete() {
        let popup = centered_rect(60, 20, area);
        frame.render_widget(
            Paragraph::new("确认删除当前书签？再次按 d 确认，Esc 取消")
                .block(Block::default().borders(Borders::ALL).title("确认")),
            popup,
        );
    } else if app.is_editing() {
        let popup = centered_rect(60, 20, area);
        let title = app.edit_prompt().unwrap_or("编辑");
        let hint = match title {
            "新建分类" => "输入新分类名，回车提交，Esc 取消",
            "重命名分类" => "输入新名称，回车提交，Esc 取消",
            "移动到分类" => "输入目标分类名，回车提交，Esc 取消",
            _ => "输入新名称，回车提交，Esc 取消",
        };
        let popup_inner = centered_rect(58, 12, popup);
        frame.render_widget(
            Paragraph::new(app.edit_text())
                .block(Block::default().borders(Borders::ALL).title(title)),
            popup,
        );
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
            popup_inner,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_categories(frame: &mut Frame<'_>, app: &App, layout: ViewLayout) {
    let rows = usize::from(layout.category_content().height);
    let offset = app.category_offset();
    let items = app
        .categories()
        .iter()
        .skip(offset)
        .take(rows)
        .map(|category| ListItem::new(category.as_str()))
        .collect::<Vec<_>>();
    let border_style = pane_border_style(app.active_pane() == Pane::Categories);
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("分类")
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if rows > 0
        && app.selected_category() >= offset
        && app.selected_category() < offset.saturating_add(rows)
    {
        state.select(Some(app.selected_category() - offset));
    }
    frame.render_stateful_widget(list, layout.categories, &mut state);
}

fn render_bookmarks(frame: &mut Frame<'_>, app: &App, layout: ViewLayout) {
    let rows = usize::from(layout.bookmark_content().height);
    let offset = app.bookmark_offset();
    let border_style = pane_border_style(app.active_pane() == Pane::Bookmarks);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("书签")
        .border_style(border_style);

    if app.visible_bookmarks().is_empty() {
        let message = if app.search_text().is_empty() {
            "还没有书签"
        } else {
            "没有匹配结果"
        };
        frame.render_widget(Paragraph::new(message).block(block), layout.bookmarks);
        return;
    }

    let items = app
        .visible_bookmarks()
        .iter()
        .skip(offset)
        .take(rows)
        .map(|bookmark| {
            let invalid = if bookmark.valid_target {
                String::new()
            } else {
                " [失效]".to_owned()
            };
            ListItem::new(format!(
                "{}{} — {}",
                bookmark.name,
                invalid,
                bookmark.path.display()
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if let Some(selected) = app.selected_bookmark()
        && rows > 0
        && selected >= offset
        && selected < offset.saturating_add(rows)
    {
        state.select(Some(selected - offset));
    }
    frame.render_stateful_widget(list, layout.bookmarks, &mut state);
}

fn pane_border_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn bordered_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::model::{Bookmark, Database};
    use crate::ui::app::{Action, App};
    use chrono::{TimeZone, Utc};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn empty_database_renders_empty_state() {
        let mut app = App::from_database_at(Database::default(), test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("还没有书签"), "{buffer}");
    }

    #[test]
    fn search_without_matches_renders_no_match_state() {
        let database = database_with_bookmark("alpha", PathBuf::from("missing-alpha"));
        let mut app = App::from_database_at(database, test_now());
        app.handle(Action::StartSearch, test_now());
        for character in "no-such-bookmark-987654321".chars() {
            app.handle(Action::Input(character), test_now());
        }

        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("没有匹配结果"), "{buffer}");
    }

    #[test]
    fn missing_path_is_visibly_marked_invalid() {
        let database = database_with_bookmark("missing", PathBuf::from("missing-directory"));
        let mut app = App::from_database_at(database, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("失效"), "{buffer}");
        assert!(buffer.contains("missing"), "{buffer}");
    }

    #[test]
    fn regular_file_is_visibly_marked_invalid() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let database = database_with_bookmark("regular-file", file.path().to_path_buf());
        let mut app = App::from_database_at(database, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("失效"), "{buffer}");
        assert!(buffer.contains("regular-file"), "{buffer}");
    }

    #[test]
    fn status_message_is_rendered_in_the_footer() {
        let database = database_with_bookmark("missing", PathBuf::from("missing-directory"));
        let mut app = App::from_database_at(database, test_now());
        app.handle(Action::Confirm, test_now());
        let buffer = render_to_string(&mut app, 80, 16);
        assert!(buffer.contains("目录不存在或不是目录"), "{buffer}");
    }

    #[test]
    fn narrow_and_short_terminals_do_not_panic() {
        for (width, height) in [(1, 1), (4, 2), (10, 3)] {
            let mut app = App::from_database_at(Database::default(), test_now());
            let _ = render_to_string(&mut app, width, height);
        }
    }

    fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .replace(' ', "")
    }

    fn database_with_bookmark(name: &str, path: PathBuf) -> Database {
        Database {
            version: 1,
            categories: vec!["default".into()],
            bookmarks: vec![Bookmark {
                id: Uuid::new_v4(),
                name: name.into(),
                path,
                category: "default".into(),
                created_at: test_now(),
                last_visited_at: None,
                visit_count: 0,
            }],
        }
    }

    fn test_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }
}
