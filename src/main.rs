mod frontmatter;
mod post;
use post::Post;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

fn load_posts<P: AsRef<Path>>(src: P) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let mut posts: Vec<Post> = Vec::default();

    for entry in src.as_ref().read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if let Some("md") = path.extension().map(std::ffi::OsStr::to_str).flatten() {
            let name = path.file_stem().map(std::ffi::OsStr::to_str).flatten();
            if name.is_none() {
                continue;
            }
            match Post::load(&path) {
                Ok(Some(p)) => posts.push(p),
                Ok(None) => (),
                Err(e) => eprintln!(
                    "skipping `{}` as it failed to parse: {:?}",
                    path.display(),
                    e
                ),
            };
        }
    }
    posts.sort_by(|a, b| b.front.date.cmp(&a.front.date));
    Ok(posts)
}

fn group_posts(
    posts: &Vec<Post>,
) -> Result<HashMap<String, Vec<Post>>, Box<dyn std::error::Error>> {
    let mut map: HashMap<String, Vec<Post>> = HashMap::default();

    for post in posts.into_iter() {
        let section = post.front.section.clone();
        let entry = map.entry(section);
        let posts = entry.or_default();
        posts.push(post.clone());
    }

    for postlist in map.values_mut() {
        postlist.sort_by(|a, b| a.front.title.cmp(&b.front.title));
    }

    Ok(map)
}

fn main() {
    use rayon::prelude::*;

    let outdir: PathBuf = PathBuf::from("docs").join("posts");
    std::fs::create_dir_all(&outdir).expect("can create docs/posts/ folder");

    let style = std::fs::read_to_string(PathBuf::from("docs").join("style.css"))
        .expect("can load CSS styles");
    let katex_style = std::fs::read_to_string(PathBuf::from("docs").join("katex.css"))
        .expect("can load katex style");

    let posts = load_posts("posts").expect("can load posts from posts/ folder");
    println!("Found {} posts, rendering them...", posts.len());
    let errors: Vec<String> = posts
        .par_iter()
        .filter_map(|post| {
            let html = match post.render(&style, &katex_style) {
                Ok(h) => h,
                Err(e) => {
                    return Some(format!(
                        "failed to render `{}`: {:?}",
                        post.source.display(),
                        e
                    ));
                }
            };
            let outdir = outdir.join(&post.front.slug);
            std::fs::create_dir_all(&outdir).expect("can create dir for post");
            let outfile = outdir.join("index.html");
            std::fs::write(outfile, html).expect("can write post to index.html file");
            return None;
        })
        .collect();
    if errors.len() > 0 {
        eprintln!("Failed to render some posts:");
        for error in errors.iter() {
            eprintln!("  {}", error);
        }
    } else {
        println!("Posts rendered!");
    }

    println!("Generating index...");
    {
        let mut context = tera::Context::new();
        context.insert("title", "Kenton Hamaluik");
        let posts = group_posts(&posts).expect("failed to group posts???");
        context.insert("posts", &posts);
        context.insert("include_katex_css", &false);
        context.insert("style", &style);

        let rendered = post::TEMPLATES
            .render("index.html", &context)
            .expect("can render index");
        let mut minifier = html_minifier::HTMLMinifier::new();
        minifier.set_remove_comments(true);
        minifier.set_minify_code(false);
        minifier.digest(rendered).expect("can minify index");
        let outpath = PathBuf::from("docs").join("index.html");
        std::fs::write(outpath, minifier.get_html()).expect("can write index to index.html file");
    }
    println!("Index generated!");

    println!("Generating RSS feed...");
    {
        let channel = rss::ChannelBuilder::default()
            .namespaces({
                let mut n: BTreeMap<String, String> = BTreeMap::new();
                n.insert("atom".to_owned(), "http://www.w3.org/2005/Atom".to_owned());
                n
            })
            .title("Kenton Hamaluik".to_string())
            .link("https://blog.hamaluik.ca".to_string())
            .description("Things from my life, usually programming related".to_string())
            .language(Some("en-ca".to_string()))
            .copyright(Some(format!(
                "Copyright {}, Kenton Hamaluik",
                chrono::Local::now().format("%Y").to_string()
            )))
            .managing_editor(Some("kenton@hamaluik.ca (Kenton Hamaluik)".to_owned()))
            .webmaster(Some("kenton@hamaluik.ca (Kenton Hamaluik)".to_owned()))
            .pub_date(Some(chrono::Local::now().to_rfc2822()))
            .last_build_date(Some(chrono::Local::now().to_rfc2822()))
            .generator(Some("A roll-my-own special".to_owned()))
            .ttl(Some("1440".to_string()))
            .image(Some(
                rss::ImageBuilder::default()
                    .url("https://blog.hamaluik.ca/avatar_rss.png".to_string())
                    .title("Kenton Hamaluik".to_string())
                    .link("https://blog.hamaluik.ca".to_string())
                    .width(Some("144".to_owned()))
                    .height(Some("144".to_owned()))
                    .description(Some("Kenton Hamaluik".to_owned()))
                    .build(),
            ))
            .items(
                posts
                    .iter()
                    .map(|post| {
                        rss::ItemBuilder::default()
                            .title(Some(post.front.title.to_owned()))
                            .link(Some(format!("https://blog.hamaluik.ca{}", post.url)))
                            .description(Some(post.front.summary.to_owned()))
                            .author(Some("kenton@hamaluik.ca (Kenton Hamaluik)".to_owned()))
                            .guid(Some(
                                rss::GuidBuilder::default()
                                    .value(format!("https://blog.hamaluik.ca{}", post.url))
                                    .permalink(true)
                                    .build(),
                            ))
                            .pub_date(Some(post.front.date.to_rfc2822()))
                            .build()
                    })
                    .collect::<Vec<rss::Item>>(),
            )
            .build();
        let output = PathBuf::from("docs").join("feed.rss");
        std::fs::write(output, channel.to_string().replace("<channel>", "<channel><atom:link href=\"https://blog.hamaluik.ca/feed.rss\" rel=\"self\" type=\"application/rss+xml\" />")).expect("can write rss feed to feed.rss");
    }
    println!("RSS feed generated!");

    println!("Copying assets...");
    let outdir = PathBuf::from("docs");
    let mut paths: Vec<PathBuf> = Vec::default();
    for entry in ignore::Walk::new("assets") {
        let entry = entry.expect("can get path entry");
        if let Some(t) = entry.file_type() {
            if t.is_file() {
                if let Some("md") = entry
                    .path()
                    .extension()
                    .map(std::ffi::OsStr::to_str)
                    .flatten()
                {
                    // ignore markdown files
                } else {
                    // we found an asset to copy!
                    paths.push(entry.path().to_owned());
                }
            }
        }
    }
    paths.par_iter().for_each(|path| {
        let dest_path: PathBuf =
            outdir.join(path.iter().skip(1).map(PathBuf::from).collect::<PathBuf>());
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).expect("can create directory");
            }
        }
        std::fs::copy(path, &dest_path).expect("can copy file");
    });
}
