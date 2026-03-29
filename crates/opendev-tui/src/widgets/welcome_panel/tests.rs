use super::*;

#[test]
fn test_widget_renders_visible_output() {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    let state = WelcomePanelState::new();
    let widget = WelcomePanelWidget::new(&state)
        .version("0.1.0")
        .mode("NORMAL");

    // Simulate a typical conversation area: 100 wide, 20 tall
    let area = Rect::new(0, 0, 100, 20);
    let mut buf = Buffer::empty(area);

    widget.render(area, &mut buf);

    // Count non-space cells that were written
    let modified_cells: usize = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let cell = buf.cell((x, y)).unwrap();
            cell.symbol() != " "
        })
        .count();

    // Should have written border chars + gradient text
    assert!(
        modified_cells > 20,
        "Expected visible output, got only {modified_cells} non-space cells"
    );
}

#[test]
fn test_widget_renders_small_terminal() {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    let state = WelcomePanelState::new();
    let widget = WelcomePanelWidget::new(&state)
        .version("0.1.0")
        .mode("NORMAL");

    // Tier 1: very small (height < 5)
    let area = Rect::new(0, 0, 80, 4);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    let modified: usize = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buf.cell((x, y)).unwrap().symbol() != " ")
        .count();
    assert!(
        modified > 5,
        "Tier 1 should render gradient text, got {modified} cells"
    );
}

#[test]
fn test_render_buffer_centered_without_rain() {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    let state = WelcomePanelState::new();
    let widget = WelcomePanelWidget::new(&state)
        .version("0.1.0")
        .mode("NORMAL");

    let area = Rect::new(0, 0, 100, 20);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    // Find which rows have content
    let mut content_rows = Vec::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if buf.cell((x, y)).unwrap().symbol() != " " {
                content_rows.push(y);
                break;
            }
        }
    }
    assert!(!content_rows.is_empty(), "No visible content");
    // Box should be roughly centered (middle third of the area)
    let center = area.height / 2;
    let first = content_rows[0];
    let last = *content_rows.last().unwrap();
    assert!(
        first <= center && last >= center - 2,
        "Box not centered: rows {first}..{last} in height {}",
        area.height
    );
}

#[test]
fn test_render_buffer_with_rain() {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    let mut state = WelcomePanelState::new();
    // Initialize rain and tick a few times
    state.ensure_rain_field(60, 13);
    for _ in 0..5 {
        state.tick(100, 20);
    }
    let widget = WelcomePanelWidget::new(&state)
        .version("0.1.0")
        .mode("NORMAL");

    let area = Rect::new(0, 0, 100, 20);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    let mut content_rows = Vec::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if buf.cell((x, y)).unwrap().symbol() != " " {
                content_rows.push(y);
                break;
            }
        }
    }
    // With rain, should have content in the rain area + box area
    assert!(
        content_rows.len() > 5,
        "Expected rain + box, got {} rows",
        content_rows.len()
    );
}
