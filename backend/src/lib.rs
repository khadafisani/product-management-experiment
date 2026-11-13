pub mod config;
mod feature;
mod helper;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests {
    use crate::feature::{
        document::{entity::LineInput, validation::total},
        payment::validation::allocate,
    };
    #[test]
    fn payment_rules() {
        assert_eq!(
            allocate(1_200_000, &[1_000_000, 1_000_000]).ok(),
            Some(vec![1_000_000, 200_000])
        );
        assert!(allocate(900_000, &[1_000_000, 1_000_000]).is_err());
        assert!(allocate(1_000_000, &[1_000_000, 1_000_000]).is_err());
        assert_eq!(allocate(900_000, &[1_000_000]).ok(), Some(vec![900_000]));
        assert!(allocate(2_100_000, &[1_000_000, 1_000_000]).is_err());
        assert!(allocate(100, &[]).is_err());
    }
    #[test]
    fn validates_lines() {
        assert!(total(&[]).is_err());
        assert!(
            total(&[LineInput {
                product_id: 1,
                item_name: None,
                quantity: i64::MAX,
                price: 2
            }])
            .is_err()
        );
        assert_eq!(
            total(&[LineInput {
                product_id: 1,
                item_name: None,
                quantity: 2,
                price: 100
            }])
            .ok(),
            Some(200)
        );
        let new_line = |name: &str| LineInput {
            product_id: 0,
            item_name: Some(name.into()),
            quantity: 1,
            price: 100,
        };
        assert_eq!(
            total(&[new_line("Chair"), new_line("Desk")]).ok(),
            Some(200)
        );
        assert!(total(&[new_line("Chair"), new_line(" chair ")]).is_err());
        assert!(total(&[new_line(" ")]).is_err());
    }
}
