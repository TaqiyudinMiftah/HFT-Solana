#[cfg(not(feature = "yellowstone"))]
fn main() {
    eprintln!(
        "paper_yellowstone requires: cargo run --features yellowstone --bin paper_yellowstone -- <config.json>"
    );
    std::process::exit(2);
}

#[cfg(feature = "yellowstone")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, io};

    use hft_solana::{
        feed::yellowstone::{
            run_account_feed, YellowstoneAccountFilter, YellowstoneConfig,
        },
        paper::async_loop::run_paper_event_loop,
        paper_config::PaperConfig,
    };
    use tokio::sync::mpsc;

    let config_path = env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: paper_yellowstone <config.json>",
        )
    })?;

    let json = fs::read_to_string(&config_path)?;
    let config = PaperConfig::from_json_str(&json)?;
    let built = config.build()?;

    let endpoint = env::var("HFT_YELLOWSTONE_ENDPOINT").map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "HFT_YELLOWSTONE_ENDPOINT is required",
        )
    })?;
    let x_token = env::var("HFT_YELLOWSTONE_X_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());

    let yellowstone = YellowstoneConfig {
        endpoint,
        x_token,
        filter_name: built.filter_name.clone(),
        filter: YellowstoneAccountFilter {
            accounts: built.account_filters.clone(),
            owners: Vec::new(),
        },
    };

    eprintln!(
        "paper mode: subscribing to {} explicit accounts; live transaction submission is disabled",
        yellowstone.filter.accounts.len()
    );

    let (feed_tx, feed_rx) = mpsc::channel(built.feed_channel_capacity);
    let (opportunity_tx, mut opportunity_rx) =
        mpsc::channel(built.opportunity_channel_capacity);

    let mut feed_task = tokio::spawn(run_account_feed(yellowstone, feed_tx));
    let mut paper_task = tokio::spawn(run_paper_event_loop(
        feed_rx,
        opportunity_tx,
        built.pipeline,
    ));

    let printer_task = tokio::spawn(async move {
        while let Some(opportunity) = opportunity_rx.recv().await {
            println!(
                "PAPER_OPPORTUNITY cycle={} amount_in={} expected_out={} effective_profit={} expected_cu={} created_ns={}",
                opportunity.cycle_id,
                opportunity.amount_in,
                opportunity.expected_out,
                opportunity.expected_effective_profit,
                opportunity.expected_cu,
                opportunity.created_ns,
            );
        }
    });

    let (pipeline, output_stats) = tokio::select! {
        feed_result = &mut feed_task => {
            let feed_result = feed_result?;
            if let Err(error) = feed_result {
                paper_task.abort();
                return Err(Box::new(error) as Box<dyn std::error::Error>);
            }

            paper_task.await??
        }
        paper_result = &mut paper_task => {
            feed_task.abort();
            paper_result??
        }
    };

    printer_task.await?;

    let stats = pipeline.stats();
    eprintln!(
        "paper stopped: feed_events={} updates={} invalidations={} evaluated={} opportunities={} forwarded={} dropped={}",
        stats.feed_events,
        stats.reactor_updates,
        stats.reactor_invalidations,
        stats.pools_evaluated,
        stats.opportunities_emitted,
        output_stats.opportunities_forwarded,
        output_stats.opportunities_dropped,
    );

    Ok(())
}
