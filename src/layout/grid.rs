use std::{error::Error, fmt};

use crate::model::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridOptions {
    pub columns: Option<usize>,
    pub gap: i32,
    pub margin: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutError(String);

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LayoutError {}

/// Select a compact grid while preferring square cells.
pub fn grid_dimensions(count: usize, area: Rect) -> (usize, usize) {
    if count == 0 {
        return (0, 0);
    }
    if count == 1 {
        return (1, 1);
    }

    let area_ratio = area.width.max(1) as f64 / area.height.max(1) as f64;
    let desired_cell_ratio = 4.0 / 3.0;
    (1..=count)
        .map(|rows| {
            let columns = count.div_ceil(rows);
            let cell_ratio = area_ratio * rows as f64 / columns as f64;
            let shape_penalty = (cell_ratio / desired_cell_ratio).ln().abs();
            let empty_penalty = (rows * columns - count) as f64 / count as f64;
            ((rows, columns), shape_penalty + empty_penalty)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(dimensions, _)| dimensions)
        .unwrap()
}

pub fn arrange(count: usize, area: Rect) -> Vec<Rect> {
    arrange_with_options(count, area, GridOptions::default())
        .expect("default grid options are valid")
}

pub fn arrange_with_options(
    count: usize,
    area: Rect,
    options: GridOptions,
) -> Result<Vec<Rect>, LayoutError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if options.columns == Some(0) {
        return Err(LayoutError("columns must be greater than zero".into()));
    }
    if options.gap < 0 {
        return Err(LayoutError("gap must not be negative".into()));
    }
    if options.margin < 0 {
        return Err(LayoutError("margin must not be negative".into()));
    }

    let doubled_margin = options
        .margin
        .checked_mul(2)
        .ok_or_else(|| LayoutError("margin is too large".into()))?;
    let inner = Rect {
        x: area.x.saturating_add(options.margin),
        y: area.y.saturating_add(options.margin),
        width: area.width.saturating_sub(doubled_margin),
        height: area.height.saturating_sub(doubled_margin),
    };
    if inner.width <= 0 || inner.height <= 0 {
        return Err(LayoutError("margin leaves no usable work area".into()));
    }

    let (rows, columns) = match options.columns {
        Some(columns) => {
            let columns = columns.min(count);
            (count.div_ceil(columns), columns)
        }
        None => grid_dimensions(count, inner),
    };
    let vertical_gap = i64::from(options.gap) * rows.saturating_sub(1) as i64;
    let usable_height = i64::from(inner.height) - vertical_gap;
    if usable_height < rows as i64 {
        return Err(LayoutError("gap leaves no usable window height".into()));
    }

    (0..count)
        .map(|index| {
            let row = index / columns;
            let column = index % columns;
            let items_in_row = columns.min(count - row * columns);
            let horizontal_gap = i64::from(options.gap) * items_in_row.saturating_sub(1) as i64;
            let usable_width = i64::from(inner.width) - horizontal_gap;
            if usable_width < items_in_row as i64 {
                return Err(LayoutError("gap leaves no usable window width".into()));
            }

            let x0 = i64::from(inner.x)
                + i64::from(options.gap) * column as i64
                + usable_width * column as i64 / items_in_row as i64;
            let x1 = i64::from(inner.x)
                + i64::from(options.gap) * column as i64
                + usable_width * (column + 1) as i64 / items_in_row as i64;
            let y0 = i64::from(inner.y)
                + i64::from(options.gap) * row as i64
                + usable_height * row as i64 / rows as i64;
            let y1 = i64::from(inner.y)
                + i64::from(options.gap) * row as i64
                + usable_height * (row + 1) as i64 / rows as i64;
            Ok(Rect {
                x: x0 as i32,
                y: y0 as i32,
                width: (x1 - x0) as i32,
                height: (y1 - y0) as i32,
            })
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
    fn expected_automatic_grids_from_one_through_eight() {
        let expected = [
            (1, 1),
            (1, 2),
            (2, 2),
            (2, 2),
            (2, 3),
            (2, 3),
            (2, 4),
            (2, 4),
        ];
        for (count, dimensions) in (1..=8).zip(expected) {
            assert_eq!(grid_dimensions(count, AREA), dimensions);
        }
    }

    #[test]
    fn covers_odd_area_without_rounding_gaps() {
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

    #[test]
    fn explicit_columns_control_shape() {
        let cells = arrange_with_options(
            6,
            Rect {
                x: 0,
                y: 0,
                width: 1200,
                height: 800,
            },
            GridOptions {
                columns: Some(3),
                gap: 0,
                margin: 0,
            },
        )
        .unwrap();
        assert_eq!(
            cells[0],
            Rect {
                x: 0,
                y: 0,
                width: 400,
                height: 400
            }
        );
        assert_eq!(
            cells[5],
            Rect {
                x: 800,
                y: 400,
                width: 400,
                height: 400
            }
        );
    }

    #[test]
    fn applies_gap_and_margin() {
        let cells = arrange_with_options(
            2,
            Rect {
                x: 0,
                y: 0,
                width: 101,
                height: 51,
            },
            GridOptions {
                columns: Some(2),
                gap: 5,
                margin: 3,
            },
        )
        .unwrap();
        assert_eq!(
            cells[0],
            Rect {
                x: 3,
                y: 3,
                width: 45,
                height: 45
            }
        );
        assert_eq!(
            cells[1],
            Rect {
                x: 53,
                y: 3,
                width: 45,
                height: 45
            }
        );
    }

    #[test]
    fn rejects_invalid_geometry_options() {
        assert!(arrange_with_options(
            1,
            AREA,
            GridOptions {
                columns: Some(0),
                gap: 0,
                margin: 0
            }
        )
        .is_err());
        assert!(arrange_with_options(
            1,
            AREA,
            GridOptions {
                columns: None,
                gap: -1,
                margin: 0
            }
        )
        .is_err());
        assert!(arrange_with_options(
            1,
            AREA,
            GridOptions {
                columns: None,
                gap: 0,
                margin: 2000
            }
        )
        .is_err());
    }
}
