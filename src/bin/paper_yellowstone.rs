#[cfg(not(feature = "yellowstone"))]
fn main() {
    eprintln!(
        "paper_yellowstone requires: cargo run --features yellowstone --bin paper_yellowstone -- <config.json>"
    );
    std::process::exit(2);
}

#[cfg(feature = "yellowstone")]
fn unix_now_ns() -> Result<u64, std::io::Error> {
    use std::time::{SystemTime, UNIX_EPOCH};

    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| std::io::Error::other("system clock is before Unix epoch"))?
        .as_nanos()
        .min(u64::MAX as u128) as u64)
}

#[cfg(feature = "yellowstone")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, io};

    use hft_solana::{
        bootstrap_rpc::{apply_bootstrap_catchup, fetch_bootstrap_snapshot, BootstrapRpcConfig},
        decode::meteora_dlmm::{
            BIN_ARRAY_DISCRIMINATOR, BIN_ARRAY_LB_PAIR_OFFSET, DLMM_PROGRAM_ID,
        },
        feed::yellowstone::{
            run_account_feed_with_ready, YellowstoneAccountFilter, YellowstoneConfig,
            YellowstoneMemcmpFilter, YellowstoneScopedAccountFilter,
        },
        landing::{choose_landing_path, LandingPaperStats},
        paper::async_loop::run_paper_event_loop_with_refresh,
        paper_config::PaperConfig,
    };
    use tokio::sync::{mpsc, oneshot};

    let config_path = env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: paper_yellowstone <config.json>",
        )
    })?;

    let json = fs::read_to_string(&config_path)?;
    let config = PaperConfig::from_json_str(&json)?;
    let mut built = config.build()?;

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
        "paper mode: subscribing to {} explicit accounts and {} DLMM bin scopes; live transaction submission is disabled",
        yellowstone.filter.accounts.len(),
        yellowstone.filter.scoped.len(),
    );

    let (feed_tx, mut feed_rx) = mpsc::channel(built.feed_channel_capacity);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel(built.opportunity_channel_capacity);
    let (refresh_tx, mut refresh_rx) = mpsc::channel(256);
    let (feed_ready_tx, feed_ready_rx) = oneshot::channel();

    let mut feed_task = tokio::spawn(run_account_feed_with_ready(
        yellowstone,
        feed_tx,
        feed_ready_tx,
    ));

    tokio::select! {
        ready = feed_ready_rx => {
            ready.map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Yellowstone feed ended before subscription readiness",
                )
            })?;
        }
        feed_result = &mut feed_task => {
            match feed_result? {
                Ok(()) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Yellowstone feed ended before subscription readiness",
                    ).into());
                }
                Err(error) => {
                    return Err(Box::new(error) as Box<dyn std::error::Error>);
                }
            }
        }
    }

    let mut bootstrap_opportunities_forwarded = 0u64;
    let mut bootstrap_opportunities_dropped = 0u64;

    if let Some(rpc_url) = env::var("HFT_SOLANA_RPC_URL")
        .ok()
        .filter(|value| !value.is_empty())
    {
        let snapshot = fetch_bootstrap_snapshot(&BootstrapRpcConfig {
            rpc_url,
            explicit_accounts: built.account_filters.clone(),
            dlmm_bin_pairs: built.dlmm_bin_pairs.clone(),
        })
        .await?;

        if feed_task.is_finished() {
            match feed_task.await? {
                Ok(()) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Yellowstone feed ended during RPC bootstrap",
                    )
                    .into());
                }
                Err(error) => {
                    return Err(Box::new(error) as Box<dyn std::error::Error>);
                }
            }
        }

        // Drain only the events that were already buffered when the RPC
        // snapshot completed. Events arriving after this finite prefix remains
        // queued for the normal paper loop.
        let buffered = feed_rx.len();
        let mut buffered_events = Vec::with_capacity(buffered);
        for _ in 0..buffered {
            let Ok(event) = feed_rx.try_recv() else {
                break;
            };
            buffered_events.push(event);
        }

        let catchup = apply_bootstrap_catchup(
            &mut built.pipeline,
            snapshot,
            buffered_events,
            unix_now_ns()?,
        );

        for request in catchup.refresh_requests {
            eprintln!(
                "PAPER_DLMM_BOOTSTRAP_INCOMPLETE pool={} missing_accounts={}",
                request.pool_id,
                request.missing_accounts.len(),
            );
        }

        for opportunity in catchup.initial.opportunities {
            match opportunity_tx.try_send(opportunity) {
                Ok(()) => {
                    bootstrap_opportunities_forwarded =
                        bootstrap_opportunities_forwarded.saturating_add(1);
                }
                Err(_) => {
                    bootstrap_opportunities_dropped =
                        bootstrap_opportunities_dropped.saturating_add(1);
                }
            }
        }

        eprintln!(
            "paper bootstrap: explicit_accounts={} dlmm_bin_arrays={} context_slot={} buffered_applied={} buffered_stale={} initial_opportunities={}",
            catchup.explicit_accounts,
            catchup.scoped_dlmm_bin_arrays,
            catchup.context_slot,
            catchup.buffered_applied,
            catchup.buffered_stale,
            bootstrap_opportunities_forwarded,
        );
    } else {
        eprintln!(
            "paper bootstrap disabled: set HFT_SOLANA_RPC_URL to seed current account state before live evaluation"
        );
    }

    let landing_policy = built.landing.clone();

    let mut paper_task = tokio::spawn(run_paper_event_loop_with_refresh(
        feed_rx,
        opportunity_tx,
        refresh_tx,
        built.pipeline,
    ));

    let printer_task = tokio::spawn(async move {
        let mut landing_stats = LandingPaperStats::default();

        while let Some(opportunity) = opportunity_rx.recv().await {
            let printed_ns = unix_now_ns().unwrap_or(opportunity.created_ns);
            println!(
                "PAPER_OPPORTUNITY cycle={} amount_in={} expected_out={} effective_profit={} expected_cu={} created_ns={} output_queue_age_ns={}",
                opportunity.cycle_id,
                opportunity.amount_in,
                opportunity.expected_out,
                opportunity.expected_effective_profit,
                opportunity.expected_cu,
                opportunity.created_ns,
                printed_ns.saturating_sub(opportunity.created_ns),
            );

            if let Some(policy) = landing_policy.as_ref() {
                match choose_landing_path(
                    &opportunity,
                    policy.candidates.iter().copied(),
                    policy.config,
                ) {
                    Some(choice) => {
                        landing_stats.record_choice(choice);
                        println!(
                            "PAPER_LANDING cycle={} provider={:?} success_probability_bps={} priority_fee={} relay_tip={} net_if_landed={} expected_value={}",
                            opportunity.cycle_id,
                            choice.provider,
                            choice.success_probability_bps,
                            choice.priority_fee,
                            choice.relay_tip,
                            choice.net_if_landed,
                            choice.expected_value,
                        );
                    }
                    None => {
                        landing_stats.record_skip();
                        println!(
                            "PAPER_LANDING_SKIP cycle={} effective_profit={}",
                            opportunity.cycle_id,
                            opportunity.expected_effective_profit,
                        );
                    }
                }
            }
        }

        landing_stats
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

    let landing_stats = printer_task.await?;
    refresh_printer_task.await?;

    let stats = pipeline.stats();
    let event_process_ns_avg = if output_stats.events_processed == 0 {
        0
    } else {
        output_stats.event_process_ns_total / output_stats.events_processed
    };
    eprintln!(
        "paper stopped: feed_events={} updates={} invalidations={} dirty_queued={} dirty_collapsed={} queue_full={} evaluated={} opportunities={} forwarded={} dropped={} bootstrap_forwarded={} bootstrap_dropped={} refresh_forwarded={} refresh_dropped={} loop_events={} event_process_ns_total={} event_process_ns_avg={} event_process_ns_max={} landing_evaluated={} landing_selected={} landing_skipped={} landing_direct={} landing_jito={} landing_helius={} landing_ev_total={}",
        stats.feed_events,
        stats.reactor_updates,
        stats.reactor_invalidations,
        stats.dirty_queued,
        stats.dirty_collapsed,
        stats.queue_full_fallbacks,
        stats.pools_evaluated,
        stats.opportunities_emitted,
        output_stats.opportunities_forwarded,
        output_stats.opportunities_dropped,
        bootstrap_opportunities_forwarded,
        bootstrap_opportunities_dropped,
        output_stats.refresh_requests_forwarded,
        output_stats.refresh_requests_dropped,
        output_stats.events_processed,
        output_stats.event_process_ns_total,
        event_process_ns_avg,
        output_stats.event_process_ns_max,
        landing_stats.evaluated,
        landing_stats.selected,
        landing_stats.skipped,
        landing_stats.direct,
        landing_stats.jito,
        landing_stats.helius_sender,
        landing_stats.expected_value_total,
    );

    Ok(())
}
