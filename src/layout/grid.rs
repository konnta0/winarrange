use crate::model::Rect;

/// Select a compact grid while preferring cells near a conventional window aspect ratio.
pub fn grid_dimensions(count: usize, area: Rect) -> (usize, usize) {
    if count == 0 {
        return (0, 0);
    }
    if count == 1 {
        return (1, 1);
    }

    let area_ratio = area.width.max(1) as f64 / area.height.max(1) as f64;
    let desired_cell_ratio = 1.0;
    (1..=count)
        .map(|rows| {
            let columns = count.div_ceil(rows);
            let cell_ratio = area_ratio * rows as f64 / columns as f64;
            let shape_penalty = (cell_ratio / desired_cell_ratio).ln().abs();
            let empty_penalty = (rows * columns - count) as f64 / count as f64 * 0.35;
            ((rows, columns), shape_penalty + empty_penalty)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(dimensions, _)| dimensions)
        .unwrap()
}

pub fn arrange(count: usize, area: Rect) -> Vec<Rect> {
    let (rows, columns) = grid_dimensions(count, area);
    if count == 0 {
        return Vec::new();
    }

    (0..count)
        .map(|index| {
            let row = index / columns;
            let column = index % columns;
            // A short final row still uses the full work area. For example,
            // three windows become two on top and one full-width window below.
            let items_in_row = columns.min(count - row * columns);
            let x0 = area.x + (area.width as i64 * column as i64 / items_in_row as i64) as i32;
            let x1 =
                area.x + (area.width as i64 * (column + 1) as i64 / items_in_row as i64) as i32;
            let y0 = area.y + (area.height as i64 * row as i64 / rows as i64) as i32;
            let y1 = area.y + (area.height as i64 * (row + 1) as i64 / rows as i64) as i32;
            Rect {
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 25,
        width: 2560,
        height: 1415,
    };

    #[test]
    fn expected_common_grids() {
        assert_eq!(grid_dimensions(1, AREA), (1, 1));
        assert_eq!(grid_dimensions(2, AREA), (1, 2));
        assert_eq!(grid_dimensions(4, AREA), (2, 2));
        assert_eq!(grid_dimensions(5, AREA), (2, 3));
        assert_eq!(grid_dimensions(8, AREA), (2, 4));
    }

    #[test]
    fn covers_area_without_rounding_gaps() {
        let cells = arrange(
            4,
            Rect {
                x: -3,
                y: 7,
                width: 101,
                height: 99,
            },
        );
        assert_eq!(
            cells[0],
            Rect {
                x: -3,
                y: 7,
                width: 50,
                height: 49
            }
        );
        assert_eq!(
            cells[3],
            Rect {
                x: 47,
                y: 56,
                width: 51,
                height: 50
            }
        );
    }

    #[test]
    fn short_last_row_uses_full_width() {
        let area = Rect {
            x: 10,
            y: 20,
            width: 1200,
            height: 800,
        };
        let cells = arrange(3, area);
        assert_eq!(cells[2].x, area.x);
        assert_eq!(cells[2].width, area.width);

        let cells = arrange(5, area);
        assert_eq!(cells[3].width, 600);
        assert_eq!(cells[4].x, 610);
    }
}
