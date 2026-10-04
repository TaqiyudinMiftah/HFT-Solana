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
        decode::meteora_dlmm::{
            BIN_ARRAY_DISCRIMINATOR, BIN_ARRAY_LB_PAIR_OFFSET, DLMM_PROGRAM_ID,
        },
        feed::yellowstone::{
            run_account_feed, YellowstoneAccountFilter, YellowstoneConfig,
            YellowstoneMemcmpFilter, YellowstoneScopedAccountFilter,
        },
        paper::async_loop::run_paper_event_loop_with_refresh,
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
            scoped: built
                .dlmm_bin_pairs
                .iter()
                .enumerate()
                .map(|(index, lb_pair)| YellowstoneScopedAccountFilter {
                    name: format!("dlmm-bin-{index}"),
                    owners: vec![DLMM_PROGRAM_ID.to_owned()],
                    memcmp: vec![
                        YellowstoneMemcmpFilter {
                            offset: 0,
                            bytes: BIN_ARRAY_DISCRIMINATOR.to_vec(),
                        },
                        YellowstoneMemcmpFilter {
                            offset: BIN_ARRAY_LB_PAIR_OFFSET,
                            bytes: lb_pair.to_vec(),
                        },
                    ],
                })
                .collect(),
        },
    };

    eprintln!(
        "paper mode: subscribing to {} explicit accounts; live transaction submission is disabled",
        yellowstone.filter.accounts.len()
    );

    let (feed_tx, feed_rx) = mpsc::channel(built.feed_channel_capacity);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel(built.opportunity_channel_capacity);
    let (refresh_tx, mut refresh_rx) = mpsc::channel(256);

    let mut feed_task = tokio::spawn(run_account_feed(yellowstone, feed_tx));
    let mut paper_task = tokio::spawn(run_paper_event_loop_with_refresh(
        feed_rx,
        opportunity_tx,
        refresh_tx,
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

    let refresh_printer_task = tokio::spawn(async move {
        while let Some(request) = refresh_rx.recv().await {
            let missing = request
                .missing_accounts
                .into_iter()
                .map(|account| solana_pubkey::Pubkey::new_from_array(account).to_string())
                .collect::<Vec<_>>()
                .join(",");

            eprintln!(
                "PAPER_DLMM_REFRESH_REQUIRED pool={} missing_accounts={}",
                request.pool_id, missing
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
    refresh_printer_task.await?;

    let stats = pipeline.stats();
    eprintln!(
        "paper stopped: feed_events={} updates={} invalidations={} evaluated={} opportunities={} forwarded={} dropped={} refresh_forwarded={} refresh_dropped={}",
        stats.feed_events,
        stats.reactor_updates,
        stats.reactor_invalidations,
        stats.pools_evaluated,
        stats.opportunities_emitted,
        output_stats.opportunities_forwarded,
        output_stats.opportunities_dropped,
        output_stats.refresh_requests_forwarded,
        output_stats.refresh_requests_dropped,
    );

    Ok(())
}
