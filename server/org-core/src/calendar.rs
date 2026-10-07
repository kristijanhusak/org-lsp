//! The calendar Org's date prompt shows, drawn as Emacs's
//! `calendar-generate` draws it: three months side by side, the one with
//! the selected day in the middle, weeks starting on Sunday. It can show
//! that month alone too, drawn the same way.
//!
//! The layout follows Emacs's defaults: a left margin of five columns,
//! three columns for each day, four between months
//! (`calendar-left-margin`, `calendar-column-width`,
//! `calendar-intermonth-spacing`), and the month and year centred above
//! each month. `tests/calendar/emacs.txt` holds what Emacs draws.

use crate::date::{days_in_month, weekday, Date, Day, MONTH_NAMES, WEEKDAY_ABBREVIATIONS};

const LEFT_MARGIN: usize = 5;
const COLUMN_WIDTH: usize = 3;
/// `calendar-month-width`: seven days and the space between months.
const MONTH_WIDTH: usize = 7 * COLUMN_WIDTH + 4;
/// `calendar-month-digit-width`: the width the header is centred in.
const MONTH_DIGIT_WIDTH: usize = 7 * COLUMN_WIDTH - 1;
/// The lines before the first week: the month and the day names.
const HEADER_LINES: usize = 2;

/// Where a day stands in the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub day: Day,
    pub line: usize,
    /// The column of the day's first digit; a day takes two columns.
    pub column: usize,
}

/// Three months of the calendar, or one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// The year and month in the middle (`displayed-month`).
    pub year: i64,
    pub month: i64,
    /// How many months it shows: 3, or 1 for the middle one alone.
    pub months: usize,
    pub lines: Vec<String>,
    pub cells: Vec<Cell>,
}

/// `calendar-increment-month`
fn add_months(year: i64, month: i64, by: i64) -> (i64, i64) {
    let months = year * 12 + month - 1 + by;
    (months.div_euclid(12), months.rem_euclid(12) + 1)
}

impl Calendar {
    /// The calendar around `month` of `year` (`calendar-generate`).
    pub fn new(year: i64, month: i64) -> Calendar {
        Calendar::with_months(year, month, 3)
    }

    /// The calendar of `months` months around `month` of `year`: 3, or 1
    /// for that month alone.
    pub fn with_months(year: i64, month: i64, months: usize) -> Calendar {
        let months = if months == 1 { 1 } else { 3 };
        let width = LEFT_MARGIN + months * MONTH_WIDTH;
        let mut grid: Vec<Vec<char>> = Vec::new();
        let mut cells = Vec::new();
        let put = |grid: &mut Vec<Vec<char>>, line: usize, column: usize, text: &str| {
            while grid.len() <= line {
                grid.push(vec![' '; width]);
            }
            for (offset, c) in text.chars().enumerate() {
                grid[line][column + offset] = c;
            }
        };
        for index in 0..months {
            let (year, month) = add_months(year, month, index as i64 - (months as i64 - 1) / 2);
            let indent = LEFT_MARGIN + index * MONTH_WIDTH;
            let header = format!("{} {year}", MONTH_NAMES[month as usize - 1]);
            let spare = MONTH_DIGIT_WIDTH.saturating_sub(header.chars().count());
            put(&mut grid, 0, indent + spare / 2, &header);
            for (day, name) in WEEKDAY_ABBREVIATIONS.iter().enumerate() {
                put(&mut grid, 1, indent + day * COLUMN_WIDTH, &name[..2]);
            }
            let first = Date::new(year, month, 1).to_day();
            let blank = weekday(first);
            for number in 1..=days_in_month(year, month) {
                let slot = blank + number as usize - 1;
                let (line, column) = (HEADER_LINES + slot / 7, indent + slot % 7 * COLUMN_WIDTH);
                put(&mut grid, line, column, &format!("{number:2}"));
                cells.push(Cell { day: first + number - 1, line, column });
            }
        }
        let lines = grid.into_iter().map(|line| line.into_iter().collect::<String>().trim_end().to_string()).collect();
        Calendar { year, month, months, lines, cells }
    }

    /// The calendar of `months` months that shows `day`, centred on its
    /// month.
    pub fn around(day: Day, months: usize) -> Calendar {
        let date = Date::from_day(day);
        Calendar::with_months(date.year, date.month, months)
    }

    /// Whether `day` is one of the days shown.
    pub fn shows(&self, day: Day) -> bool {
        self.cell(day).is_some()
    }

    pub fn cell(&self, day: Day) -> Option<Cell> {
        self.cells.iter().find(|cell| cell.day == day).copied()
    }

    /// The day at `column` of `line`: on its digits or the blank after.
    pub fn day_at(&self, line: usize, column: usize) -> Option<Day> {
        let cell = self
            .cells
            .iter()
            .find(|cell| cell.line == line && (cell.column..cell.column + COLUMN_WIDTH).contains(&column))?;
        Some(cell.day)
    }

    /// The same calendar `by` months later (`calendar-scroll-left`).
    pub fn scrolled(&self, by: i64) -> Calendar {
        let (year, month) = add_months(self.year, self.month, by);
        Calendar::with_months(year, month, self.months)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_what_emacs_draws() {
        let reference = include_str!("../../../tests/calendar/emacs.txt");
        let mut blocks = 0;
        for block in reference.split("== ").filter(|block| !block.is_empty()) {
            let (title, body) = block.split_once('\n').unwrap();
            let (month, year) = title.split_once(' ').unwrap();
            let calendar = Calendar::new(year.trim().parse().unwrap(), month.parse().unwrap());
            let expected: Vec<&str> = body.trim_end_matches('\n').lines().collect();
            assert_eq!(calendar.lines, expected, "around {month}/{year}");
            blocks += 1;
        }
        assert_eq!(blocks, 7);
    }

    #[test]
    fn finds_days_by_place() {
        let calendar = Calendar::new(2026, 10);
        let day = Date::new(2026, 10, 5).to_day();
        let cell = calendar.cell(day).unwrap();
        assert_eq!((cell.line, cell.column), (3, 33));
        assert_eq!(&calendar.lines[3][33..35], " 5");
        assert_eq!(calendar.day_at(3, 33), Some(day));
        assert_eq!(calendar.day_at(3, 35), Some(day));
        assert_eq!(calendar.day_at(0, 10), None);
        assert!(calendar.shows(Date::new(2026, 9, 1).to_day()));
        assert!(!calendar.shows(Date::new(2026, 12, 1).to_day()));
        assert_eq!((calendar.scrolled(3).year, calendar.scrolled(3).month), (2027, 1));
    }

    #[test]
    fn draws_one_month_alone() {
        let three = Calendar::new(2026, 10);
        let one = Calendar::with_months(2026, 10, 1);
        // The middle month of the three, moved to the left margin.
        let middle: Vec<String> = three
            .lines
            .iter()
            .map(|line| format!("{:5}{}", "", line.get(5 + MONTH_WIDTH..).unwrap_or("")).trim_end().to_string())
            .map(|line| line.chars().take(5 + MONTH_WIDTH).collect::<String>().trim_end().to_string())
            .collect();
        assert_eq!(one.lines, middle);
        assert_eq!(one.lines[0], "         October 2026");
        let day = Date::new(2026, 10, 5).to_day();
        assert_eq!(one.cell(day).map(|cell| (cell.line, cell.column)), Some((3, 8)));
        assert!(!one.shows(Date::new(2026, 9, 30).to_day()));
        let next = one.scrolled(1);
        assert_eq!((next.year, next.month, next.months), (2026, 11, 1));
        assert_eq!(Calendar::around(day, 1).months, 1);
    }
}
