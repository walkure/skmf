use chrono::{Date, Datelike, TimeZone};
use chrono_tz::{Asia::Tokyo, Tz};

#[derive(serde_derive::Deserialize, Debug)]
pub struct SkUser {
    pub user: String,
    pub pass: String,
}

#[derive(Debug)]
pub struct SkDatum {
    pub date: Date<Tz>,
    pub price: u32,
    pub shop: String,
    pub menu: String,
}

pub enum SkDataType {
    /// 残高入金履歴
    PaymentHistory,
    /// 購入履歴
    PrepaidHistory,
}

pub fn parse_sk_csv(year: i32, data: &str, dtype: SkDataType) -> Result<Vec<SkDatum>, String> {
    match data.split_once("\r\n") {
        None => {
            return Err("split error".to_string());
        }
        Some((_, csv)) => {
            let mut reader = csv::Reader::from_reader(csv.as_bytes());
            let mut v = Vec::new();
            for result in reader.records() {
                let record = result.unwrap();
                let datum = SkDatum {
                    date: parse_sk_date(year, &record[0])?,
                    shop: record[1].to_string(),
                    menu: kana::combine(&kana::half2full(&record[2])),
                    price: dparse(match dtype {
                        SkDataType::PaymentHistory => &record[3],
                        SkDataType::PrepaidHistory => &record[4],
                    })?,
                };
                v.push(datum);
            }
            return Ok(v);
        }
    }
}

fn dparse(d: &str) -> Result<u32, String> {
    return d
        .parse::<u32>()
        .map_err(|e| format!("error:{:?} value:{}", e.kind(), d));
}

fn parse_sk_date(year: i32, data: &str) -> Result<Date<Tz>, String> {
    // "7/1(金)"
    //let (day,_) = data.split_once("/").and_then(|(_,d)| d.split_once("(")).ok_or("parse failure")?;
    let (month, dx) = data.split_once("/").ok_or("parse failure")?;
    let (day, _) = dx.split_once("(").ok_or("parse failure")?;

    let month = dparse(month).map_err(|e| format!("month err:{}", e))?;
    let day = dparse(day).map_err(|e| format!("day err:{}", e))?;

    let dt = Tokyo.ymd(year, month as u32, day as u32);

    return Ok(dt);
}

pub fn get_sk_agent(user: SkUser) -> Result<ureq::Agent, String> {
    let agent = ureq::agent();
    /*
    let proxy = ureq::Proxy::new("localhost:8888").map_err(|e| e.to_string())?;
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    let agent = ureq::builder()
        .redirects(20)
        .tls_connector(std::sync::Arc::new(tls))
        .proxy(proxy)
        .build();
    // */
    let resp = agent
        .post("https://mp.seikyou.jp/mypage-sp/Auth.login.do")
        .send_form(&[("loginId", &user.user), ("password", &user.pass)])
        .map_err(|e| format!("http err:{}", e))?;

    if let None = resp.header("Set-Cookie") {
        return Ok(agent);
    }
    return Err("login failure!".to_string());
}

pub fn get_sk_history(
    agent: &ureq::Agent,
    date: Date<Tz>,
    dtype: SkDataType,
) -> Result<Vec<SkDatum>, String> {
    let resp = agent
        .post(match dtype {
            SkDataType::PaymentHistory => {
                "https://mp.seikyou.jp/mypage-sp/PaymentHistory.csvDownload.do"
            }
            SkDataType::PrepaidHistory => {
                "https://mp.seikyou.jp/mypage-sp/PrepaidHistory.csvDownload.do"
            }
        })
        .send_form(&[("rirekiDate", &date.format("%Y年%m月").to_string())])
        .map_err(|err| format!("failure to get csv:{:?}", err))?;
    if resp.status() != 200 {
        return Err(format!("resp:{:?}", resp.into_string()));
    };

    let data = resp
        .into_string()
        .map_err(|e| format!("encode err:{:?}", e))?;
    return parse_sk_csv(date.year(), &data, dtype);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::prelude::*;
    #[test]
    fn payment_csv_test() {
        let data = sk_load_file(2021, SkDataType::PaymentHistory);

        assert_eq!(data[0].date, Tokyo.ymd(2021, 6, 29));
        assert_eq!(data[0].shop, "京大ルネＤ");
        assert_eq!(data[0].price, 1000);
    }

    #[test]
    fn dparse_test() {
        let result = dparse("12345");
        assert_eq!(result, Ok(12345));
        let result = dparse("a1234");
        assert_eq!(result, Err("error:InvalidDigit value:a1234".to_string()));
    }

    #[test]
    fn prepaid_csv_test() {
        let data = sk_load_file(2022, SkDataType::PrepaidHistory);

        assert_eq!(data[0].date, Tokyo.ymd(2022, 7, 19));
        assert_eq!(data[0].shop, "京大ルネＤ");
        assert_eq!(data[0].menu, "唐揚げカレーM/ほうれん草");
        assert_eq!(data[0].price, 473);
    }

    fn sk_load_file(year: i32, dtype: SkDataType) -> Vec<SkDatum> {
        let fname = match dtype {
            SkDataType::PaymentHistory => "./src/testdata/paymentHistory_20220724.csv",
            SkDataType::PrepaidHistory => "./src/testdata/prepaidHistory_20220720.csv",
        };
        let mut f = File::open(fname).expect("file not found");
        let mut contents = String::new();
        f.read_to_string(&mut contents)
            .expect("something went wrong reading the file");
        return parse_sk_csv(year, &contents, dtype).unwrap();
    }
}
