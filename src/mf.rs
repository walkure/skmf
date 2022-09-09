use chrono::Datelike;
use cookie_store::CookieStore;
use parsercher;
use parsercher::dom::Dom;
use parsercher::dom::DomType;
use parsercher::dom::Tag;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;

use url::Url;

use chrono::Date;
use chrono::TimeZone;
use chrono_tz::{Asia::Tokyo, Tz};

#[derive(serde_derive::Deserialize, Debug)]
pub struct MfUser {
    pub email: String,
    pub pass: String,
}

#[derive(Debug)]
pub struct MfSession {
    agent: ureq::Agent,
    csrf_token: String,
    accounts: HashMap<String, String>,
    subaccounts: HashMap<String, String>,
    categories: HashMap<String, MfAccountCategory>,
}

pub fn save_mf_session(session: MfSession) -> Result<(), String> {
    let mut file = BufWriter::new(File::create("cookies.json").map_err(|e| e.to_string())?);
    session
        .agent
        .cookie_store()
        .save_json(&mut file)
        .map_err(|e| e.to_string())?;

    return Ok(());
}

pub fn get_mf_session(user: MfUser) -> Result<MfSession, String> {
    let store = match File::open("cookies.json") {
        Ok(f) => {
            let file = BufReader::new(f);
            CookieStore::load_json(file).map_err(|e| e.to_string())?
        }
        Err(_e) => CookieStore::default(),
    };

    let agent = ureq::builder().cookie_store(store).redirects(10).build();
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
        .cookie_store(store)
        .build();
    // */
    let res = agent
        .get("https://moneyforward.com/")
        .call()
        .map_err(|e| e.to_string())?;
    let html = res.into_string().map_err(|e| e.to_string())?;
    let root_dom = parsercher::parse(&html).map_err(|e| e.to_string())?;

    match html.find("グループの追加・編集") {
        Some(_) => {
            return Ok(MfSession {
                agent,
                csrf_token: get_csrf_token(&root_dom)?,
                accounts: get_accounts(&root_dom)?,
                subaccounts: get_subaccounts(&root_dom)?,
                categories: get_account_types(&root_dom)?,
            });
        }
        None => create_mf_session(agent, user),
    }
}

fn create_mf_session(agent: ureq::Agent, user: MfUser) -> Result<MfSession, String> {
    // get client info
    let res = agent
        .get("https://moneyforward.com/sign_in")
        .call()
        .map_err(|e| e.to_string())?;

    // get login form URL
    let login = get_login_url(res.get_url())?;

    // get email login form
    let res = agent
        .get(login.as_str())
        .call()
        .map_err(|e| e.to_string())?;

    let queries = get_url_queries(res.get_url())?;
    let mut queries = queries
        .iter()
        .map(|e| ((*e.0).as_str(), (*e.1).as_str()))
        .collect::<HashMap<_, _>>();

    let html = res.into_string().map_err(|e| e.to_string())?;
    let root_dom = parsercher::parse(&html).map_err(|e| e.to_string())?;
    let csrf_token = get_csrf_token(&root_dom)?;

    queries.insert("authenticity_token", csrf_token.as_str());
    queries.insert("_method", "post");

    queries.insert("mfid_user[email]", user.email.as_str());
    queries.insert("hiddenPassword", "");
    queries.insert("authenticator_response", "");

    let queries: Vec<_> = queries.iter().map(|(k, v)| (*k, *v)).collect();

    // set email mode
    let res = agent
        .post("https://id.moneyforward.com/sign_in/email")
        .send_form(&queries[..])
        .map_err(|e| e.to_string())?;
    let url = res.get_url();

    // send login request
    let queries = get_url_queries(url)?;
    let mut queries = queries
        .iter()
        .map(|e| ((*e.0).as_str(), (*e.1).as_str()))
        .collect::<HashMap<_, _>>();

    let html = res.into_string().map_err(|e| e.to_string())?;
    let root_dom = parsercher::parse(&html).map_err(|e| e.to_string())?;

    let csrf_token = get_csrf_token(&root_dom)?;
    queries.insert("authenticity_token", &csrf_token);
    queries.insert("_method", "post");
    queries.insert("mfid_user[email]", user.email.as_str());
    queries.insert("mfid_user[password]", user.pass.as_str());

    let queries: Vec<_> = queries.iter().map(|(k, v)| (*k, *v)).collect();
    let res = agent
        .post("https://id.moneyforward.com/sign_in")
        .send_form(&queries[..])
        .map_err(|e| e.to_string())?;
    let html = res.into_string().map_err(|e| e.to_string())?;

    html.find("グループの追加・編集").ok_or("cannot login")?;
    let root_dom = parsercher::parse(&html).map_err(|e| e.to_string())?;

    return Ok(MfSession {
        agent,
        csrf_token: get_csrf_token(&root_dom)?,
        accounts: get_accounts(&root_dom)?,
        subaccounts: get_subaccounts(&root_dom)?,
        categories: get_account_types(&root_dom)?,
    });
}

fn get_csrf_token(root_dom: &Dom) -> Result<String, String> {
    let mut csrf_token_key = Tag::new("meta");
    csrf_token_key.set_attr("name", "csrf-token");
    let texts =
        parsercher::search_tag(&root_dom, &csrf_token_key).ok_or("cannot find csrf token tag")?;
    if texts.len() == 0 {
        return Err("cannot find token".to_string());
    }
    let csrf_token = texts[0].get_attr("content").ok_or("cannot find token")?;

    return Ok(csrf_token);
}

fn get_url_queries(url: &str) -> Result<HashMap<String, String>, String> {
    let target = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let mut queries = HashMap::new();

    let _: Vec<_> = target
        .query_pairs()
        .map(|q| queries.insert(q.0.to_string(), q.1.to_string()))
        .collect();

    return Ok(queries);
}

fn get_login_url(url: &str) -> Result<Url, String> {
    let target = url::Url::parse(&url).map_err(|e| e.to_string())?;

    let queries = target.query();
    let mut login_url =
        Url::parse("https://id.moneyforward.com/sign_in/email").map_err(|e| e.to_string())?;
    login_url.set_query(queries);

    return Ok(login_url);
}

fn get_subaccounts(root_dom: &Dom) -> Result<HashMap<String, String>, String> {
    let mut needle_tag = Tag::new("select");
    needle_tag.set_attr("name", "user_asset_act[sub_account_id_hash]");
    needle_tag.set_attr("id", "user_asset_act_sub_account_id_hash");
    let mut needle = parsercher::dom::Dom::new(parsercher::dom::DomType::Tag);
    needle.set_tag(needle_tag);

    let mut subaccounts = HashMap::new();

    if let Some(doms) = parsercher::search_dom(&root_dom, &needle) {
        for dom in doms
            .get_children()
            .ok_or("failure to extract root DOM")?
            .get(0)
            .ok_or("no value tag exists")?
            .get_children()
            .ok_or("failure to extract value DOM")?
        {
            let tag = dom.get_tag().ok_or("failure to fetch tag")?;
            let id = tag.get_attr("value").ok_or("failure to fetch value")?;
            let nametag = dom
                .get_children()
                .ok_or("failure to nametag")?
                .get(0)
                .ok_or("no name tag exists")?;
            let name = nametag
                .get_text()
                .ok_or("failure to fetch name")?
                .get_text()
                .trim()
                .to_string();

            if id == "0" {
                continue;
            }
            subaccounts.insert(name, id);
        }
        return Ok(subaccounts);
    }
    return Err("falure to fetch subaccounts".to_string());
}

#[derive(Debug)]
struct MfAccountCategory {
    name: String,
    id: String,
    subcategory: HashMap<String, String>,
}

fn get_account_types(root_dom: &Dom) -> Result<HashMap<String, MfAccountCategory>, &str> {
    let mut needle_tag = Tag::new("li");
    needle_tag.set_attr("class", "dropdown-submenu");
    let mut needle = Dom::new(DomType::Tag);
    needle.set_tag(needle_tag);

    let mut categories = HashMap::new();

    if let Some(doms) = parsercher::search_dom(&root_dom, &needle) {
        for dom in doms.get_children().unwrap() {
            let category = get_category(dom)?;
            categories.insert(category.name.clone(), category);
        }
        return Ok(categories);
    }
    return Err("dropdown not found");
}

fn get_category(dom: &Dom) -> Result<MfAccountCategory, &'static str> {
    let mut needle_tag = Tag::new("a");
    needle_tag.set_attr("class", "l_c_name");
    let mut needle = Dom::new(DomType::Tag);
    needle.set_tag(needle_tag);

    if let Some(at) = parsercher::search_dom(&dom, &needle) {
        let id = at
            .get_children()
            .ok_or("broken category html(1)")?
            .get(0)
            .ok_or("broken category html(2)")?
            .get_tag()
            .ok_or("broken category html(3)")?
            .get_attr("id")
            .ok_or("broken category html(4)")?;

        let text = at
            .get_children()
            .ok_or("broken category html(5)")?
            .get(0)
            .ok_or("broken category html(6)")?
            .get_children()
            .ok_or("broken category html(7)")?
            .get(0)
            .ok_or("broken category html(8)")?
            .get_text()
            .ok_or("broken category html(9)")?
            .get_text();

        return Ok(MfAccountCategory {
            name: text.to_string(),
            id,
            subcategory: get_subcategory(dom).unwrap(),
        });
    }
    return Err("notag");
}

fn get_subcategory(dom: &Dom) -> Result<HashMap<String, String>, &'static str> {
    let mut needle_tag = Tag::new("ul");
    needle_tag.set_attr("class", "dropdown-menu sub_menu");
    let mut needle = Dom::new(DomType::Tag);
    needle.set_tag(needle_tag);

    let mut subcategory = HashMap::new();

    if let Some(at) = parsercher::search_dom(&dom, &needle) {
        for entity in at
            .get_children()
            .ok_or("broken subcategory html(1)")?
            .get(0)
            .ok_or("broken subcategory html(2)")?
            .get_children()
            .ok_or("broken subcategory html(3)")?
        {
            if entity
                .get_tag()
                .ok_or("broken subcategory html(4)")?
                .get_name()
                != "li"
            {
                continue;
            }
            let it = entity
                .get_children()
                .ok_or("broken subcategory html(5)")?
                .get(0)
                .ok_or("broken subcategory html(6)")?;
            if it
                .get_tag()
                .ok_or("broken subcategory html(7)")?
                .get_attr("class")
                .ok_or("broken subcategory html(8)")?
                != "m_c_name"
            {
                continue;
            }

            let id = it
                .get_tag()
                .ok_or("broken subcategory html(9)")?
                .get_attr("id")
                .ok_or("broken subcategory html(a)")?;
            let name = it
                .get_children()
                .ok_or("broken subcategory html(b)")?
                .get(0)
                .ok_or("broken subcategory html(c)")?
                .get_text()
                .ok_or("broken subcategory html(d)")?
                .get_text();
            subcategory.insert(name.to_string(), id);
        }
        return Ok(subcategory);
    }
    return Err("dame");
}

#[derive(Debug)]
pub struct MfDatum {
    pub target: bool,
    pub date: Date<Tz>,
    pub content: String,
    pub price: i32,
    pub bank: String,
    pub category: String,
    pub subcategory: String,
    pub memo: String,
    pub transfer: bool,
    pub id: String,
}

pub fn get_history(
    session: &MfSession,
    account: &str,
    date: Date<Tz>,
) -> Result<Vec<MfDatum>, String> {
    let account_id_hash = session
        .accounts
        .get(account)
        .ok_or("account name unknown")?;

    let url = format!(
        "https://moneyforward.com/cf/csv?account_id_hash={0}&year={1}&month={2}",
        account_id_hash,
        date.year(),
        date.month()
    );

    let result = session
        .agent
        .get(&url)
        .call()
        .map_err(|e| format!("failure to get mf csv:{}", e))?;

    if result.content_type() != "text/csv" {
        return Err("invalid data type".to_string());
    }

    let mut data = Vec::<MfDatum>::new();

    // server returns with false charset.
    let body = get_encoded_string(&mut result.into_reader(), "Shift_JIS")?;

    let enc = kana::half2full(&body);
    let mut reader = csv::Reader::from_reader(enc.as_bytes());
    for record in reader.records() {
        let record = record.map_err(|e| format!("csv data broken:{}", e))?;
        let it = MfDatum {
            target: record[0].eq("1"),
            date: parse_date(&record[1])?,
            content: record[2].to_string(),
            price: record[3]
                .parse::<i32>()
                .map_err(|e| format!("invalid price data type:{}", e.to_string()))?,
            bank: record[4].to_string(),
            category: record[5].to_string(),
            subcategory: record[6].to_string(),
            memo: record[7].to_string(),
            transfer: record[8].eq("1"),
            id: record[9].to_string(),
        };
        data.push(it);
    }

    return Ok(data);
}

use encoding_rs::Encoding;
use std::io::Read;
fn get_encoded_string(rdr: &mut impl std::io::Read, charset: &str) -> Result<String, String> {
    let mut buf: Vec<u8> = vec![];
    let encoding = Encoding::for_label(charset.as_bytes()).ok_or("unknown encoding")?;

    let _ = rdr
        .take(4194304)
        .read_to_end(&mut buf)
        .map_err(|e| format!("ioerr:{:?}", e.to_string()));
    let (text, _, _) = encoding.decode(&buf);

    return Ok(text.into_owned());
}

fn parse_date(date: &str) -> Result<Date<Tz>, String> {
    // 2022/07/26
    let datum: Vec<&str> = date.split("/").collect();
    let year = datum
        .get(0)
        .ok_or("invalid year data")?
        .parse::<i32>()
        .map_err(|e| format!("invaid year data:{:?}", e.to_string()))?;

    let month = datum
        .get(1)
        .ok_or("invalid month data")?
        .parse::<u32>()
        .map_err(|e| format!("invaid month data:{:?}", e.to_string()))?;

    let day = datum
        .get(2)
        .ok_or("invalid day data")?
        .parse::<u32>()
        .map_err(|e| format!("invaid day data:{:?}", e.to_string()))?;

    let dt = Tokyo.ymd(year, month, day);

    return Ok(dt);
}

#[derive(Debug)]
pub struct MfAssetDatum<'a> {
    ///振替
    pub is_transfer: bool,
    /// 収入
    pub is_income: bool,
    /// 振替時の出金元
    pub sub_account_from: &'a str,
    /// 振替時の入金先
    pub sub_account_to: &'a str,
    /// 更新日時
    pub updated_at: Date<Tz>,
    /// 金額
    pub amount: i32,
    /// 出金対象
    pub sub_account: &'a str,
    /// 内容
    pub content: &'a str,
    /// 大分類
    pub large_category: &'a str,
    /// 中分類
    pub middle_category: &'a str,
}

pub fn send_datum(session: &MfSession, datum: MfAssetDatum) -> Result<(), String> {
    let mut formdatum = Vec::new();

    let updated_at = datum.updated_at.format("%Y/%m/%d").to_string();
    let amount = format!("{}", datum.amount);

    let category = session.categories.get(datum.large_category).ok_or(format!(
        "large category [{}] not found",
        datum.large_category
    ))?;

    let large_category_id = &category.id;
    let middle_category_id = category
        .subcategory
        .get(datum.middle_category)
        .ok_or(format!(
            "middle account [{}] not found",
            datum.middle_category
        ))?;

    let sub_account_id_hash = session
        .subaccounts
        .get(datum.sub_account)
        .ok_or(format!("sub account [{}] not found", datum.sub_account))?;

    let sub_account_id_hash_from = if datum.sub_account_from != "" {
        session
            .subaccounts
            .get(datum.sub_account_from)
            .ok_or(format!(
                "subaccount from[{}] not found",
                datum.sub_account_from
            ))?
    } else {
        ""
    };
    let sub_account_id_hash_to = if datum.sub_account_to != "" {
        session
            .subaccounts
            .get(datum.sub_account_to)
            .ok_or(format!("subaccount to[{}] not found", datum.sub_account_to))?
    } else {
        ""
    };

    formdatum.push((
        "user_asset_act[is_transfer]",
        if datum.is_transfer { "1" } else { "0" },
    ));
    formdatum.push((
        "user_asset_act[is_income]",
        if datum.is_income { "1" } else { "0" },
    ));
    formdatum.push(("user_asset_act[payment]", "2"));
    formdatum.push((
        "user_asset_act[sub_account_id_hash_from]",
        sub_account_id_hash_from,
    ));
    formdatum.push((
        "user_asset_act[sub_account_id_hash_to]",
        sub_account_id_hash_to,
    ));
    formdatum.push(("user_asset_act[updated_at]", &updated_at));
    formdatum.push(("user_asset_act[recurring_limit_off_flag]", "0"));
    formdatum.push(("user_asset_act[recurring_rule_only_flag]", "0"));
    formdatum.push(("user_asset_act[amount]", &amount));
    formdatum.push(("user_asset_act[sub_account_id_hash]", &sub_account_id_hash));
    formdatum.push(("user_asset_act[large_category_id]", large_category_id));
    formdatum.push(("user_asset_act[middle_category_id]", middle_category_id));
    formdatum.push(("user_asset_act[content]", &datum.content));

    let _ = session
        .agent
        .post("https://moneyforward.com/user_asset_acts")
        .set("x-csrf-token", &session.csrf_token)
        .send_form(&formdatum[..])
        .map_err(|e| format!("http error:{}", e.to_string()))?;

    Ok(())
}

fn get_accounts(root_dom: &Dom) -> Result<HashMap<String, String>, String> {
    let mut needle_tag = Tag::new("li");
    needle_tag.set_attr("class", "account facilities-column border-bottom-dotted");
    let mut needle = parsercher::dom::Dom::new(parsercher::dom::DomType::Tag);
    needle.set_tag(needle_tag);

    let account_doms = parsercher::search_dom(&root_dom, &needle).ok_or("account not found")?;

    let mut needle_tag = Tag::new("p");
    needle_tag.set_attr("class", "heading-accounts");
    let mut needle = parsercher::dom::Dom::new(parsercher::dom::DomType::Tag);
    needle.set_tag(needle_tag);

    let heading_doms = parsercher::search_dom(&account_doms, &needle).ok_or("heading not found")?;

    let mut accounts = HashMap::new();

    for ch in heading_doms
        .get_children()
        .ok_or("heading children not found")?
    {
        let anchor = &ch
            .get_children()
            .ok_or("anchor children not found")?
            .get(0)
            .ok_or("anchor not found")?;

        let href = anchor
            .get_tag()
            .ok_or("anchor tag not found")?
            .get_attr("href")
            .ok_or("anchor href not found")?;
        let name = anchor
            .get_children()
            .ok_or("anchor child dom not found")?
            .get(0)
            .ok_or("anchor child object not found")?
            .get_text()
            .ok_or("anchor text not found")?
            .get_text();

        let account_id = href.split("/").collect::<Vec<_>>();
        let account_id = account_id.get(3).ok_or("account path invalid")?;
        accounts.insert(name.to_string(), account_id.to_string());
    }

    return Ok(accounts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_subaccounts_test() {
        let input = r#"
<div class='source-container'>
<label class='source-label'>支出元</label>
<select name="user_asset_act[sub_account_id_hash]" id="user_asset_act_sub_account_id_hash"><option value="4BnmknyROGt5jY7W1B2AgFuObztWcn1">財布   </option>
<option selected="selected" value="fcLyOp1rcKRZVA23oO8oIfx3BL01rX3n2">大学生協   </option>
<option value="ozLKkB5b30MvdQ2xcTr4NrhQ39k573Wa">ヨドバシカード   </option>
<option value="0">なし</option></select>
        "#;
        let root_dom = parsercher::parse(input).unwrap();
        let result = get_subaccounts(&root_dom).unwrap();
        assert!(result.len() == 3);
        assert_eq!(result["大学生協"], "fcLyOp1rcKRZVA23oO8oIfx3BL01rX3n2");
        assert_eq!(result["ヨドバシカード"], "ozLKkB5b30MvdQ2xcTr4NrhQ39k573Wa");
        assert_eq!(result["財布"], "4BnmknyROGt5jY7W1B2AgFuObztWcn1");
    }

    #[test]
    fn get_csrf_token_test() {
        let input = r#"
<meta name="csrf-token" content="iMPAh10Er0Kt38BEshIGs8Zk429lpaCx+1VA3btRJS8qPcjSMSOdEkBj0OWA1OnZUPWKqb7QrTKmb9" />
        "#;
        let root_dom = parsercher::parse(input).unwrap();
        let result = get_csrf_token(&root_dom);
        assert_eq!(
            result.unwrap(),
            "iMPAh10Er0Kt38BEshIGs8Zk429lpaCx+1VA3btRJS8qPcjSMSOdEkBj0OWA1OnZUPWKqb7QrTKmb9"
        );
    }

    #[test]
    fn get_account_types_test() {
        let input = r#"
<ul class='dropdown-menu main_menu minus'>
<li class='dropdown-submenu'>
<a class='l_c_name' id='11'>食費</a>
<ul class='dropdown-menu sub_menu' id='11'>
<span class='js-middle-category-add-area-class-11'></span>
<li>
<a class='m_c_name' id='41'>食料品</a>
</li>
<li>
<a class='m_c_name' id='42'>外食</a>
</li>
<li style='margin: 3px 0px 10px 20px;'>
<div class='js-new-middle-category-form' data-url='/middle_categories/create' id='middle-category-form-11'>
<input class="middle_category_add input-medium js-dropdown-off js-middle-category-name" placeholder="項目を追加" type="text" name="middle_category_11[name]" id="middle_category_11_name" />
<a class="anchor-color-off js-middle-category-add-icon" href="?"><i class="icon-save js-middle-category-add-icon" style="padding: 0px 10px;"></i></a>
<input value="11" class="js-middle-category-large-category-id" autocomplete="off" type="hidden" name="middle_category_11[large_category_id]" id="middle_category_11_large_category_id" />
<input value="true" class="js-middle-category-transaction-page" autocomplete="off" type="hidden" name="middle_category_11[transaction_page]" id="middle_category_11_transaction_page" />
<input value="1" class="js-middle-category-reload-type" autocomplete="off" type="hidden" name="middle_category_11[reload_type]" id="middle_category_11_reload_type" />
</div>
</li>
</ul>
</li>
<li class='dropdown-submenu'>
<a class='l_c_name' id='10'>日用品</a>
<ul class='dropdown-menu sub_menu' id='10'>
<span class='js-middle-category-add-area-class-10'></span>
<li>
<a class='m_c_name' id='36'>日用品</a>
</li>
<li>
<a class='m_c_name' id='46'>子育て用品</a>
</li>
<li>
<a class='m_c_name' id='37'>ドラッグストア</a>
</li>
<li style='margin: 3px 0px 10px 20px;'>
<div class='js-new-middle-category-form' data-url='/middle_categories/create' id='middle-category-form-10'>
<input class="middle_category_add input-medium js-dropdown-off js-middle-category-name" placeholder="項目を追加" type="text" name="middle_category_10[name]" id="middle_category_10_name" />
<a class="anchor-color-off js-middle-category-add-icon" href="?"><i class="icon-save js-middle-category-add-icon" style="padding: 0px 10px;"></i></a>
<input value="10" class="js-middle-category-large-category-id" autocomplete="off" type="hidden" name="middle_category_10[large_category_id]" id="middle_category_10_large_category_id" />
<input value="true" class="js-middle-category-transaction-page" autocomplete="off" type="hidden" name="middle_category_10[transaction_page]" id="middle_category_10_transaction_page" />
<input value="1" class="js-middle-category-reload-type" autocomplete="off" type="hidden" name="middle_category_10[reload_type]" id="middle_category_10_reload_type" />
</div>
</li>
</ul>
</li>


        "#;
        let root_dom = parsercher::parse(input).unwrap();
        let result = get_account_types(&root_dom).unwrap();

        let d = &result["日用品"];
        assert_eq!(d.id, "10");
        assert_eq!(d.name, "日用品");
        assert_eq!(d.subcategory["ドラッグストア"], "37");
        assert_eq!(d.subcategory["日用品"], "36");
        assert_eq!(d.subcategory["子育て用品"], "46");

        let d = &result["食費"];
        assert_eq!(d.id, "11");
        assert_eq!(d.name, "食費");
        assert_eq!(d.subcategory["食料品"], "41");
        assert_eq!(d.subcategory["外食"], "42");
    }

    #[test]
    fn get_accounts_test() {
        let input = r#"
<section class="accounts" id="registered-manual-accounts"><h2 class="title">手元の現金を登録・管理</h2>
<div class="clearfix" style="margin-top: 10px;"><div class="pull-right"><a class="btn btn-small" href="/accounts/new/wallet">財布を作成</a></div>
</div><ul class="facilities accounts-list"><li class="edit-link-wrapper" id="top"></li><li class="heading-category-name heading-normal">財布（現金管理）</li>
<li class="account facilities-column border-bottom-dotted"><p class="heading-accounts">
<a href="/accounts/show_manual/mEAiuPmpxuah1kCUuCTNGjHDC2DOoQW">財布</a></p><ul><li class="number">1円</li>
<li class="edit-links"><a href="/accounts/edit_manual/mEAiuPmpxuah1kCUuCTNGjHDC2DOoQW">編集</a></li></ul></li>
<li class="heading-category-name heading-normal">カード</li><li class="account facilities-column border-bottom-dotted">
<p class="heading-accounts"><a href="/accounts/show_manual/1TD5ieGgTJi47Us30pemlTVclkgc7BG3Kq">ヨドバシカード</a></p><ul>
<li class="number">0円</li><li class="edit-links"><a href="/accounts/edit_manual/1TD5ieGgTJi47Us30pemlTVclkgc7BG3Kq">編集</a></li></ul>
</li><li class="heading-category-name heading-normal">電子マネー・プリペイド</li><li class="account facilities-column border-bottom-dotted">
<p class="heading-accounts"><a href="/accounts/show_manual/GESAT1R0F0E8WMoP8K34DIcVqZo8M79JhfbG">大学生協</a></p><ul>
<li class="number">11円</li><li class="edit-links"><a href="/accounts/edit_manual/GESAT1R0F0E8WMoP8K34DIcVqZo8M79JhfbG">編集</a></li></ul>
</li></ul></section>
        "#;
        let root_dom = parsercher::parse(input).unwrap();

        let result = get_accounts(&root_dom).unwrap();

        assert_eq!(result["大学生協"], "GESAT1R0F0E8WMoP8K34DIcVqZo8M79JhfbG");
        assert_eq!(result["財布"], "mEAiuPmpxuah1kCUuCTNGjHDC2DOoQW");
        assert_eq!(
            result["ヨドバシカード"],
            "1TD5ieGgTJi47Us30pemlTVclkgc7BG3Kq"
        );
    }
}
