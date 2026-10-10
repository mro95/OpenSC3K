//! The city model and what starts one. See `docs/sim/new-city.md`.

use crate::dirt::Terrain;

/// `cSC3NewCityInfo` (libSimInit): what `cSC3CmdNewCity::Execute` collects from the New City
/// dialog and hands to `cSC3City::Init(cISC3NewCityInfo&)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCityInfo {
    /// Windows-1252 bytes.
    pub city_name: Vec<u8>,
    pub mayor_name: Vec<u8>,
    pub funds: i64,
    /// The funds are borrowed (Hard).
    pub funds_are_debt: bool,
    /// Map size in cells. `SetCitySize` sets all three to the same value.
    pub x_size: u32,
    pub y_size: u32,
    pub z_size: u32,
    pub start_year: u32,
    /// 1 easy, 2 medium, 3 hard.
    pub difficulty: i32,
    /// Never set on the new-city path, so always the constructor's 7. Meaning unknown.
    pub city_type: i32,
    pub auto_budget: bool,
    pub disasters: bool,
    /// The dirt generator's output (`+0x48`). The New City dialog generates it when OK is
    /// pressed, before the city exists.
    pub terrain: Option<Terrain>,
}

impl Default for NewCityInfo {
    /// `cSC3NewCityInfo::cSC3NewCityInfo` (Loki libSimInit 0x4B364).
    /// Unchecked: no Windows address known yet.
    fn default() -> NewCityInfo {
        NewCityInfo {
            city_name: b"New City".to_vec(),
            mayor_name: b"Defacto".to_vec(),
            funds: 0,
            funds_are_debt: false,
            x_size: 0x80,
            y_size: 0x80,
            z_size: 0x80,
            start_year: 1900,
            difficulty: 0,
            city_type: 7,
            auto_budget: false,
            disasters: false,
            terrain: None,
        }
    }
}

impl NewCityInfo {
    /// `SetCitySize`: sets the X, Y and Z sizes together.
    pub fn set_city_size(&mut self, size: u32) {
        self.x_size = size;
        self.y_size = size;
        self.z_size = size;
    }
}

/// A calendar date, as `cRZDate` is built from (month, day, year).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: u32,
    pub month: u32,
    pub day: u32,
}

/// The city, as far as `cSC3City::Init(cISC3NewCityInfo&)` sets it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct City {
    pub name: Vec<u8>,
    pub mayor: Vec<u8>,
    /// Map size in cells.
    pub x_size: u32,
    pub y_size: u32,
    pub z_size: u32,
    pub city_type: i32,
    pub date: Date,
    pub difficulty: i32,
    pub disasters: bool,
    pub auto_budget: bool,
    /// Money in the treasury (`cSC3BudgetLayer` total funds).
    pub funds: i64,
    pub bonds: Vec<Bond>,
    /// What the terrain layer (`cSC3DirtBag`) is built from in `BaseInitLayers`. `None`
    /// when the info carried no generator; what the original does then is not traced.
    pub terrain: Option<Terrain>,
}

/// One bond (`cSC3BudgetLayer`, 28 bytes each). Only issuing is ported; repayment belongs to
/// the budget simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bond {
    /// Month of issue.
    pub month: u32,
    /// Year the bond is paid off: the issue year + `BOND_LIFESPAN`.
    pub end_year: u32,
    pub amount: u32,
    /// Total to repay: lifespan × 12 × monthly payment.
    pub total: u32,
    pub remaining: u32,
    /// Paid so far. Set to 0 at issue; its use is not traced.
    pub paid: u32,
    pub monthly_payment: u32,
}

/// `cSC3BudgetLayer` constants (Loki libSimMisc).
pub const MAX_BONDS: usize = 10;
pub const MAX_BOND_AMOUNT: u32 = 25_000;
pub const BOND_LIFESPAN: u32 = 10;
pub const MONTHLY_PAYMENT_PER_1K: u32 = 15;

impl City {
    /// `cSC3City::Init(cISC3NewCityInfo&)` (Loki libSimCity 0x292C4), in its order:
    /// 1. Sizes and city type.
    /// 2. The date, 1 January of the start year.
    /// 3. `Init(x, y, z, dirt generator)`. This builds the layers; the terrain comes in
    ///    through the dirt generator.
    /// 4. Name, mayor, difficulty, disasters and auto budget.
    /// 5. Funds. With debt the treasury is set to 0 and the funds come from `IssueBond`.
    /// Unchecked: no Windows address known yet.
    pub fn new(info: &NewCityInfo) -> City {
        let mut city = City {
            name: info.city_name.clone(),
            mayor: info.mayor_name.clone(),
            x_size: info.x_size,
            y_size: info.y_size,
            z_size: info.z_size,
            city_type: info.city_type,
            date: Date {
                year: info.start_year,
                month: 1,
                day: 1,
            },
            difficulty: info.difficulty,
            disasters: info.disasters,
            auto_budget: info.auto_budget,
            funds: 0,
            bonds: Vec::new(),
            terrain: info.terrain.clone(),
        };
        if info.funds_are_debt {
            city.issue_bond(info.funds.clamp(0, u32::MAX as i64) as u32);
        } else {
            city.funds = info.funds;
        }
        city
    }

    /// `cSC3BudgetLayer::IssueBond` (Loki libSimMisc 0x6C098). The amount is rounded down to
    /// a multiple of 5,000. The bond is refused if:
    /// - `MAX_BONDS` bonds are already out;
    /// - the rounded amount is 0 or above `MAX_BOND_AMOUNT`;
    /// - the total borrowed would pass the borrowing limit. That limit is not ported yet, so
    ///   it is not checked.
    ///
    /// On success the amount is deposited.
    /// Unchecked: no check runs SIMMISC.DLL yet.
    pub fn issue_bond(&mut self, amount: u32) -> bool {
        let amount = amount / 5000 * 5000;
        if self.bonds.len() >= MAX_BONDS || amount == 0 || amount > MAX_BOND_AMOUNT {
            return false;
        }
        let monthly_payment = amount * MONTHLY_PAYMENT_PER_1K / 1000;
        let total = BOND_LIFESPAN * monthly_payment * 12;
        self.bonds.push(Bond {
            month: self.date.month,
            end_year: self.date.year + BOND_LIFESPAN,
            amount,
            total,
            remaining: total,
            paid: 0,
            monthly_payment,
        });
        self.funds += amount as i64;
        true
    }

    /// `GetTotalBorrowed`.
    pub fn total_borrowed(&self) -> i64 {
        self.bonds.iter().map(|b| b.amount as i64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_info() {
        let info = NewCityInfo::default();
        assert_eq!((info.x_size, info.y_size, info.z_size), (0x80, 0x80, 0x80));
        assert_eq!((info.start_year, info.city_type, info.funds), (1900, 7, 0));
    }

    #[test]
    fn loan_city() {
        let mut info = NewCityInfo {
            funds: 10_000,
            funds_are_debt: true,
            difficulty: 3,
            start_year: 2000,
            ..NewCityInfo::default()
        };
        info.set_city_size(0x40);
        let city = City::new(&info);
        assert_eq!((city.funds, city.total_borrowed()), (10_000, 10_000));
        let bond = city.bonds[0];
        assert_eq!(
            (bond.monthly_payment, bond.total, bond.end_year),
            (150, 18_000, 2010)
        );
        assert_eq!(
            city.date,
            Date {
                year: 2000,
                month: 1,
                day: 1
            }
        );
        assert_eq!((city.x_size, city.y_size, city.z_size), (0x40, 0x40, 0x40));
    }

    #[test]
    fn cash_city() {
        let info = NewCityInfo {
            funds: 50_000,
            ..NewCityInfo::default()
        };
        let city = City::new(&info);
        assert_eq!((city.funds, city.total_borrowed()), (50_000, 0));
    }

    #[test]
    fn bond_limits() {
        let mut city = City::new(&NewCityInfo::default());
        assert!(!city.issue_bond(4_999), "rounds down to 0");
        assert!(!city.issue_bond(30_000));
        assert!(city.issue_bond(7_500));
        assert_eq!(city.funds, 5_000);
        for _ in 1..MAX_BONDS {
            assert!(city.issue_bond(5_000));
        }
        assert!(!city.issue_bond(5_000));
    }
}
