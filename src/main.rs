mod mf;
mod sk;
use chrono::Date;
use chrono::Datelike;

use chrono::TimeZone;
use chrono::Utc;
use chrono_tz::{Asia::Tokyo, Tz};
use argh::FromArgs;
use mf::send_datum;

#[derive(serde_derive::Deserialize, Debug)]
struct Config {
    mf: mf::MfUser,
    sk: sk::SkUser,
    skmf: SkMfConfig,
}

#[derive(serde_derive::Deserialize, Debug)]
struct SkMfConfig {
    mf_subaccount: String,
    mf_large_category: String,
    mf_middle_category: String,
    mf_subaccount_from: String,
    mf_charge_large_category: String,
    mf_charge_middle_category: String,
}

#[derive(Debug,FromArgs)]
/// skmf: seikyo to moneyforward data transporter
struct Args{

    #[argh(option, default = "String::from(\"config.toml\")")]
    /// path for config file. default value is "config.toml"
    config:String,
}

fn main() {
    match do_main() {
        Ok(_) => {}
        Err(msg) => println!("Error:{}", msg),
    }
}

fn do_main() -> Result<(), String> {
    let arg:Args = argh::from_env();
    println!("using config:{}",arg.config);

    let data =
        std::fs::read_to_string(&arg.config).map_err(|e| format!("conf[{}] load err:{}",arg.config,e))?;
    let conf: Config = toml::from_str(&data).map_err(|e| format!("conf load err:{}", e))?;

    let mfs = mf::get_mf_session(conf.mf)?;
    let ska = sk::get_sk_agent(conf.sk)?;

    let date = get_date(Tokyo);
    println!("start(1) at {}", date);
    if let Err(e) = send_skmf(&mfs, &ska, date, &conf.skmf) {
        mf::save_mf_session(mfs)?;
        return Err(e);
    }

    let date = get_past_date(date);
    println!("start(2) at {}", date);
    if let Err(e) = send_skmf(&mfs, &ska, date, &conf.skmf) {
        mf::save_mf_session(mfs)?;
        return Err(e);
    }

    mf::save_mf_session(mfs)?;
    Ok(())
}

fn get_date(tz: Tz) -> Date<Tz> {
    let utcdate = Utc::today().naive_utc();
    let date = tz.from_utc_date(&utcdate);

    tz.ymd(date.year(), date.month(), 1)
}

fn get_past_date(dt: Date<Tz>) -> Date<Tz> {
    let mut year = dt.year();
    let mut month = dt.month();
    let tz = dt.timezone();

    if month <= 1 {
        month = 12;
        year = year - 1;
    } else {
        month = month - 1;
    }

    tz.ymd(year, month, 1)
}

fn send_skmf(
    mfs: &mf::MfSession,
    ska: &ureq::Agent,
    date: Date<Tz>,
    skmf: &SkMfConfig,
) -> Result<(), String> {
    let mfd = mf::get_history(&mfs, &skmf.mf_subaccount, date)?;
    let prepaid = sk::get_sk_history(&ska, date, sk::SkDataType::PrepaidHistory)?;
    let payment = sk::get_sk_history(&ska, date, sk::SkDataType::PaymentHistory)?;

    let mut i = 0;

    for it in get_skmf_diff(&mfd, prepaid, sk::SkDataType::PrepaidHistory) {
        let datum = mf::MfAssetDatum {
            is_transfer: false,
            is_income: false,
            sub_account_from: "",
            sub_account_to: "",
            updated_at: it.date,
            amount: it.price as i32,
            sub_account: &skmf.mf_subaccount,
            content: &it.menu,
            large_category: &skmf.mf_large_category,
            middle_category: &skmf.mf_middle_category,
        };
        send_datum(&mfs, datum)?;
        i = i + 1;
    }
    println!("prepaid. send {} records", i);
    i = 0;

    for it in get_skmf_diff(&mfd, payment, sk::SkDataType::PaymentHistory) {
        let datum = mf::MfAssetDatum {
            is_transfer: true,
            is_income: false,
            sub_account_from: &skmf.mf_subaccount_from,
            sub_account_to: &skmf.mf_subaccount,
            updated_at: it.date,
            amount: it.price as i32,
            sub_account: &skmf.mf_subaccount,
            content: &it.menu,
            large_category: &skmf.mf_charge_large_category,
            middle_category: &skmf.mf_charge_middle_category,
        };
        send_datum(&mfs, datum)?;
        i = i + 1;
    }
    println!("payment. send {} records", i);

    Ok(())
}

use std::collections::HashSet;
fn get_skmf_diff(
    mfdata: &Vec<mf::MfDatum>,
    skdata: Vec<sk::SkDatum>,
    skdtype: sk::SkDataType,
) -> Vec<sk::SkDatum> {
    let mut filtered = Vec::<sk::SkDatum>::new();

    let mut watched = HashSet::<&String>::new();

    'skloop: for it in skdata {
        for c in mfdata {
            let price = match skdtype {
                sk::SkDataType::PrepaidHistory => {
                    if c.price > 0 {
                        continue;
                    }
                    (c.price * -1) as u32
                }
                sk::SkDataType::PaymentHistory => {
                    if c.price < 0 {
                        continue;
                    }
                    c.price as u32
                }
            };

            if watched.contains(&c.id) {
                continue;
            }

            if it.date == c.date && it.price == price && it.menu == c.content {
                watched.insert(&c.id);
                continue 'skloop;
            }
        }
        filtered.push(it);
    }
    return filtered;
}

#[cfg(test)]
mod tests {
    use crate::{mf::MfDatum, sk::SkDatum};

    use super::*;

    use sk;

    #[test]
    fn test_diff_prepaid() {
        let mf_dummy = vec![
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 10), "menu1", -120, "id1"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 11), "menu2", -123, "id2"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 11), "menu2", -123, "id3"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 13), "menu2", -123, "id4"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 13), "menu3", -125, "id5"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 14), "menu1", -120, "id6"),
        ];

        let sk_dummy = vec![
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 10), "menu1", 120), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "menu2", 123), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "menu2", 123), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "menu2", 123),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 13), "menu2", 123), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 13), "menu3", 125), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "menu2", 123),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "menu1", 120), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "menu4", 129),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 15), "menu4", 129),
        ];

        let sk_want_result = vec![
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "menu2", 123),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "menu2", 123),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "menu4", 129),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 15), "menu4", 129),
        ];

        let result = get_skmf_diff(&mf_dummy, sk_dummy, sk::SkDataType::PrepaidHistory);

        for (i, it) in result.iter().enumerate() {
            assert!(compare_sk(it, &sk_want_result[i]));
        }
    }

    #[test]
    fn test_diff_payment() {
        let mf_dummy = vec![
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 10), "", 1000, "id1"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 11), "", 1000, "id2"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 11), "", 1000, "id3"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 13), "", 1000, "id4"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 13), "", 1000, "id5"),
            make_dummy_mfdatum(Tokyo.ymd(2022, 7, 14), "", 1000, "id6"),
        ];

        let sk_dummy = vec![
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 10), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 13), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 13), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "", 1000), // registered
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 15), "", 1000),
        ];

        let sk_want_result = vec![
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 11), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 14), "", 1000),
            make_dummy_skdatum(Tokyo.ymd(2022, 7, 15), "", 1000),
        ];

        let result = get_skmf_diff(&mf_dummy, sk_dummy, sk::SkDataType::PaymentHistory);

        for (i, it) in result.iter().enumerate() {
            assert!(compare_sk(it, &sk_want_result[i]));
        }
    }

    #[test]
    fn get_past_date_test() {
        let today = Tokyo.ymd(2020, 3, 1);
        let result = get_past_date(today);
        assert_eq!(result, Tokyo.ymd(2020, 2, 1));

        let today = Tokyo.ymd(2020, 1, 1);
        let result = get_past_date(today);
        assert_eq!(result, Tokyo.ymd(2019, 12, 1));
    }

    fn compare_sk(i: &SkDatum, j: &SkDatum) -> bool {
        return i.date == j.date && i.menu == j.menu && i.price == j.price && i.shop == j.shop;
    }

    fn make_dummy_mfdatum(date: Date<Tz>, content: &str, price: i32, id: &str) -> MfDatum {
        return MfDatum {
            target: true,
            date: date,
            content: content.to_string(),
            price: price,
            bank: "".to_string(),
            category: "".to_string(),
            subcategory: "".to_string(),
            memo: "".to_string(),
            transfer: false,
            id: id.to_string(),
        };
    }
    fn make_dummy_skdatum(date: Date<Tz>, content: &str, price: u32) -> SkDatum {
        return SkDatum {
            date: date,
            price: price,
            shop: "".to_string(),
            menu: content.to_string(),
        };
    }
}
