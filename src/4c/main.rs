use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use rand::Rng; 

// --- Constants ---
const INITIAL_WEIGHT: f64 = 1.0;
const MESSAGE_DELAY_MS: u64 = 100; // Simulate network delay
const MIN_WEIGHT_FRACTION: f64 = 0.1; // Smallest fraction of weight to send

// --- Message Types ---
#[derive(Debug)]
enum Message {
    Computation { sender_id: usize, weight: f64 },
    Control { sender_id: usize, weight: f64 },
}

// --- Process State ---
#[derive(Debug, PartialEq, Clone, Copy)]
enum ProcessState {
    Active,
    Idle,
}

// --- Process Struct ---
struct Process {
    id: usize,
    is_controlling_agent: bool,
    weight: f64,
    state: ProcessState,
    // Channel for sending messages to other processes (via the central dispatcher)
    outgoing_tx: Sender<(usize, Message)>,
    // Channel for receiving messages specific to this process
    incoming_rx: Receiver<Message>,
    // Sender to the controlling agent for control messages
    controlling_agent_id: usize, // ID of the controlling agent
    // For simulation: keep track of how many computation messages were sent
    // to decide when to go idle
    sent_computation_messages_count: usize,
}

impl Process {
    fn new(
        id: usize,
        is_controlling_agent: bool,
        outgoing_tx: Sender<(usize, Message)>,
        incoming_rx: Receiver<Message>,
        controlling_agent_id: usize,
    ) -> Self {
        let weight = if is_controlling_agent { INITIAL_WEIGHT } else { 0.0 };
        println!("Process {} initialized (CA: {}), weight: {:.4}", id, is_controlling_agent, weight);
        Process {
            id,
            is_controlling_agent,
            weight,
            state: ProcessState::Idle, // Initially idle
            outgoing_tx,
            incoming_rx,
            controlling_agent_id,
            sent_computation_messages_count: 0,
        }
    }

    fn set_active(&mut self) {
        if self.state == ProcessState::Idle {
            self.state = ProcessState::Active;
            println!("Process {} is now ACTIVE. Weight: {:.4}", self.id, self.weight);
        }
    }

    fn set_idle(&mut self) {
        if self.state == ProcessState::Active {
            self.state = ProcessState::Idle;
            println!("Process {} is now IDLE. Weight: {:.4}", self.id, self.weight);
            // Rule 3: If an active process becomes idle, send its weight back to the controlling agent
            if self.weight > 0.0 && !self.is_controlling_agent {
                println!(
                    "Process {} going IDLE, sending back weight {:.4} to CA ({})",
                    self.id, self.weight, self.controlling_agent_id
                );
                let _ = self.outgoing_tx.send((
                    self.controlling_agent_id,
                    Message::Control {
                        sender_id: self.id,
                        weight: self.weight,
                    },
                ));
                self.weight = 0.0; // Its own weight becomes zero after sending
            }
        }
    }

    fn send_computation_message(&mut self, receiver_id: usize, content_weight: f64) {
        let msg = Message::Computation {
            sender_id: self.id,
            weight: content_weight,
        };
        // Rule 1: A process sending a computation message splits its current weight
        self.weight -= content_weight;
        println!(
            "Process {} (Active) sending {:.4} to {}. Remaining weight: {:.4}",
            self.id, content_weight, receiver_id, self.weight
        );
        let _ = self.outgoing_tx.send((receiver_id, msg));
        self.sent_computation_messages_count += 1;
    }

    fn run(&mut self, all_process_ids: &[usize]) {
        let mut rng = rand::rng();
        print!("Process {} starting with weight {:.4} in state {:?}\n", 
            self.id, self.weight, self.state
        );
        loop {
            // Try to receive messages without blocking indefinitely
            match self.incoming_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(msg) => {
                    match msg {
                        Message::Computation { sender_id, weight } => {
                            // Rule 2: On receipt of B(DW), process P adds DW to its weight W
                            self.weight += weight;
                            println!(
                                "Process {} received COMP msg from {}, added {:.4}. New weight: {:.4}",
                                self.id, sender_id, weight, self.weight
                            );
                            if self.state == ProcessState::Idle {
                                self.set_active(); // If P idle, P becomes active.
                            }
                        }
                        Message::Control { sender_id, weight } => {
                            if self.is_controlling_agent {
                                // Rule 4: On receiving C(DW), the controlling agent adds DW to its weight
                                self.weight += weight;
                                println!(
                                    "Controlling Agent ({}) received CTRL msg from {}, added {:.4}. Total CA weight: {:.4}",
                                    self.id, sender_id, weight, self.weight
                                );
                            } else {
                                println!(
                                    "Process {} (not CA) received unexpected CTRL msg from {}",
                                    self.id, sender_id
                                );
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No message, continue with local logic
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // All senders dropped, gracefully exit thread
                    println!("Process {} incoming channel disconnected, shutting down.", self.id);
                    break;
                }
            }

            // Simulate work and sending messages
            if self.state == ProcessState::Active {
                if self.weight > MIN_WEIGHT_FRACTION && rng.random_bool(0.3) {
                    // 30% chance to send a message
                    let receiver_candidates: Vec<usize> =
                        all_process_ids.iter().filter(|&&pid| pid != self.id).cloned().collect();

                    if !receiver_candidates.is_empty() {
                        let receiver_index = rng.random_range(0..receiver_candidates.len());
                        let receiver_id = receiver_candidates[receiver_index];
                        let weight_fraction = rng.random_range(MIN_WEIGHT_FRACTION..1.0);
                        let sent_weight = self.weight * weight_fraction;
                        self.send_computation_message(receiver_id, sent_weight);
                    }
                }

                // Simulate becoming idle after doing some work
                if self.sent_computation_messages_count >= 2 && rng.random_bool(0.6) {
                    self.set_idle();
                    self.sent_computation_messages_count = 0; // Reset for next activation
                } else if self.weight < MIN_WEIGHT_FRACTION && !self.is_controlling_agent {
                    // If a non-CA process runs out of weight, it goes idle
                    self.set_idle();
                }
            }

            // Small sleep to simulate time passing and prevent busy-waiting
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn run_with_monitoring(
        &mut self, 
        all_process_ids: &[usize],
        ca_weight_arc: Arc<Mutex<f64>>,
        ca_state_arc: Arc<Mutex<ProcessState>>,
    ) {
        let mut rng = rand::rng();
        println!("Process {} (CA) starting with weight {:.4} in state {:?}", 
            self.id, self.weight, self.state
        );
        
        loop {
            // Try to receive messages without blocking indefinitely
            match self.incoming_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(msg) => {
                    match msg {
                        Message::Computation { sender_id, weight } => {
                            // Rule 2: On receipt of B(DW), process P adds DW to its weight W
                            self.weight += weight;
                            println!(
                                "CA ({}) received COMP msg from {}, added {:.4}. New weight: {:.4}",
                                self.id, sender_id, weight, self.weight
                            );
                            if self.state == ProcessState::Idle {
                                self.set_active(); // If P idle, P becomes active.
                            }
                            
                            // Update shared CA state
                            *ca_weight_arc.lock().unwrap() = self.weight;
                            *ca_state_arc.lock().unwrap() = self.state;
                        }
                        Message::Control { sender_id, weight } => {
                            if self.is_controlling_agent {
                                // Rule 4: On receiving C(DW), the controlling agent adds DW to its weight
                                self.weight += weight;
                                println!(
                                    "CA ({}) received CTRL msg from {}, added {:.4}. Total CA weight: {:.4}",
                                    self.id, sender_id, weight, self.weight
                                );
                                
                                // Update shared CA state
                                *ca_weight_arc.lock().unwrap() = self.weight;
                                *ca_state_arc.lock().unwrap() = self.state;
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No message, continue with local logic
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // All senders dropped, gracefully exit thread
                    println!("CA ({}) incoming channel disconnected, shutting down.", self.id);
                    break;
                }
            }

            // Simulate work and sending messages
            if self.state == ProcessState::Active {
                if self.weight > MIN_WEIGHT_FRACTION * 2.0 && rng.random_bool(0.3) {
                    // 30% chance to send a message
                    let receiver_candidates: Vec<usize> =
                        all_process_ids.iter().filter(|&&pid| pid != self.id).cloned().collect();

                    if !receiver_candidates.is_empty() {
                        let receiver_index = rng.random_range(0..receiver_candidates.len());
                        let receiver_id = receiver_candidates[receiver_index];
                        let weight_fraction = rng.random_range(MIN_WEIGHT_FRACTION..1.0);
                        let sent_weight = self.weight * weight_fraction;
                        self.send_computation_message(receiver_id, sent_weight);
                        
                        // Update shared CA state after sending
                        *ca_weight_arc.lock().unwrap() = self.weight;
                        *ca_state_arc.lock().unwrap() = self.state;
                    }
                }

                // Simulate becoming idle after doing some work
                if self.sent_computation_messages_count >= 2 && rng.random_bool(0.6) {
                    self.set_idle();
                    self.sent_computation_messages_count = 0; // Reset for next activation
                    
                    // Update shared CA state
                    *ca_weight_arc.lock().unwrap() = self.weight;
                    *ca_state_arc.lock().unwrap() = self.state;
                } else if self.weight < MIN_WEIGHT_FRACTION && !self.is_controlling_agent {
                    // If a non-CA process runs out of weight, it goes idle
                    self.set_idle();
                    
                    // Update shared CA state
                    *ca_weight_arc.lock().unwrap() = self.weight;
                    *ca_state_arc.lock().unwrap() = self.state;
                }
            }

            // Small sleep to simulate time passing and prevent busy-waiting
            thread::sleep(Duration::from_millis(50));
        }
    }
}

// --- Helper Functions for Main ---

/// Send a message to a specific process from the main function
fn send_message_from_main(
    receiver_id: usize,
    message: Message,
    process_txs: &Arc<Mutex<HashMap<usize, Sender<Message>>>>,
) -> Result<(), String> {
    let p_txs = process_txs.lock().unwrap();
    if let Some(tx) = p_txs.get(&receiver_id) {
        tx.send(message).map_err(|e| format!("Failed to send message: {}", e))?;
        Ok(())
    } else {
        Err(format!("Receiver {} not found", receiver_id))
    }
}

/// Send a computation message to initiate work
fn initiate_computation(
    sender_id: usize,
    receiver_id: usize,
    weight: f64,
    process_txs: &Arc<Mutex<HashMap<usize, Sender<Message>>>>,
) -> Result<(), String> {
    let msg = Message::Computation { sender_id, weight };
    send_message_from_main(receiver_id, msg, process_txs)?;
    println!(
        "Main: Initiated computation from {} to {} with weight {:.4}",
        sender_id, receiver_id, weight
    );
    Ok(())
}

/// Send a control message back to the controlling agent
fn send_control_to_ca(
    sender_id: usize,
    ca_id: usize,
    weight: f64,
    process_txs: &Arc<Mutex<HashMap<usize, Sender<Message>>>>,
) -> Result<(), String> {
    let msg = Message::Control { sender_id, weight };
    send_message_from_main(ca_id, msg, process_txs)?;
    println!(
        "Main: Sent control message from {} to CA ({}) with weight {:.4}",
        sender_id, ca_id, weight
    );
    Ok(())
}

// --- Main Orchestrator ---
fn main() {
    let num_processes = 5;
    let controlling_agent_id = 0; // Let process 0 be the controlling agent

    // Central dispatcher for messages: (receiver_id, Message)
    let (dispatcher_tx, dispatcher_rx): (
        Sender<(usize, Message)>,
        Receiver<(usize, Message)>,
    ) = channel();

    // Store senders for each process to allow dispatcher to send to them
    let mut process_txs: HashMap<usize, Sender<Message>> = HashMap::new();
    let mut process_handles = vec![];
    let process_ids: Arc<Vec<usize>> = Arc::new((0..num_processes).collect()); // Store all process IDs for random selection

    // For monitoring the CA's state from the main thread
    let ca_weight_arc: Arc<Mutex<f64>> = Arc::new(Mutex::new(0.0));
    let ca_state_arc: Arc<Mutex<ProcessState>> = Arc::new(Mutex::new(ProcessState::Idle));

    // Create and spawn processes
    for i in 0..num_processes {
        let (process_local_tx, process_local_rx) = channel();
        process_txs.insert(i, process_local_tx);

        let is_ca = i == controlling_agent_id;
        let outgoing_tx_clone = dispatcher_tx.clone(); // Clone for each process
        let ca_weight_clone = ca_weight_arc.clone();
        let ca_state_clone = ca_state_arc.clone();
        let process_ids_clone = Arc::clone(&process_ids);

        let handle = thread::spawn(move || {
            let mut process = Process::new(
                i,
                is_ca,
                outgoing_tx_clone,
                process_local_rx,
                controlling_agent_id,
            );

            // If it's the controlling agent, update shared state
            if is_ca {
                *ca_weight_clone.lock().unwrap() = process.weight;
                *ca_state_clone.lock().unwrap() = process.state;
            }

            // Run the process logic with CA state monitoring
            if is_ca {
                let weight_arc = ca_weight_clone.clone();
                let state_arc = ca_state_clone.clone();
                process.run_with_monitoring(&process_ids_clone, weight_arc, state_arc);
                
                // Final update of CA state
                *ca_weight_clone.lock().unwrap() = process.weight;
                *ca_state_clone.lock().unwrap() = process.state;
            } else {
                process.run(&process_ids_clone);
            }
        });
        process_handles.push(handle);
    }

    // Drop the original dispatcher_tx to ensure the dispatcher_rx eventually disconnects
    drop(dispatcher_tx);

    // --- Message Dispatcher Thread ---
    // This thread simulates the network by routing messages to the correct process
    let process_txs_arc = Arc::new(Mutex::new(process_txs));
    let dispatcher_handle = {
        let process_txs_arc = Arc::clone(&process_txs_arc);
        let dispatcher_rx = dispatcher_rx; // Move the receiver into the thread
        println!("Starting message dispatcher thread...");
        // Spawn the dispatcher thread
        thread::spawn(move || {
            for (receiver_id, message) in dispatcher_rx {
                thread::sleep(Duration::from_millis(MESSAGE_DELAY_MS)); // Simulate network delay
                let p_txs = process_txs_arc.lock().unwrap();
                if let Some(tx) = p_txs.get(&receiver_id) {
                    let _ = tx.send(message);
                } else {
                    eprintln!("Error: Receiver {} not found for message {:?}", receiver_id, message);
                }
            }
            println!("Dispatcher shutting down.");
        })
    };



    println!("\n--- Initiating Computation ---");
    
    // Method 1: Send initial computation message from CA
    let ca_initial_send_weight = INITIAL_WEIGHT * 0.5;
    let initial_receiver_id = 1; // Send to process 1 for simplicity
    
    // Create and send the initial computation message through the dispatcher
    let initial_msg = Message::Computation {
        sender_id: controlling_agent_id,
        weight: ca_initial_send_weight,
    };
    
    // Send message through the process_txs directly (simulating CA sending initial work)
    {
        let p_txs = process_txs_arc.lock().unwrap();
        if let Some(tx) = p_txs.get(&initial_receiver_id) {
            let _ = tx.send(initial_msg);
            println!(
                "Main: Sent initial computation message from CA ({}) to process {} with weight {:.4}",
                controlling_agent_id, initial_receiver_id, ca_initial_send_weight
            );
        }
    }
    
    // Update CA's state after sending initial message
    {
        let mut ca_w = ca_weight_arc.lock().unwrap();
        *ca_w -= ca_initial_send_weight;
        let mut ca_s = ca_state_arc.lock().unwrap();
        *ca_s = ProcessState::Active; // CA becomes active after initiating work
        println!(
            "Main: CA ({}) remaining weight after initial send: {:.4}",
            controlling_agent_id, *ca_w
        );
    }
    
    // Method 2: Send additional messages if needed (example)
    thread::sleep(Duration::from_millis(200)); // Wait a bit before sending more
    
    // Example: Send a control message (though this would normally come from processes)
    let example_control_msg = Message::Control {
        sender_id: 2, // Pretend process 2 is sending back weight
        weight: 0.1,
    };
    
    // Send control message to CA
    {
        let p_txs = process_txs_arc.lock().unwrap();
        if let Some(tx) = p_txs.get(&controlling_agent_id) {
            let _ = tx.send(example_control_msg);
            println!(
                "Main: Sent example control message to CA with weight 0.1"
            );
        }
    }

    // --- Monitor for Termination ---
    
    let mut consecutive_idle_checks = 0;
    const MAX_IDLE_CHECKS: usize = 5; // Number of consecutive checks CA must be idle
    
    loop {
        thread::sleep(Duration::from_millis(500)); // Check every half second
        
        let current_ca_weight = *ca_weight_arc.lock().unwrap();
        let current_ca_state = *ca_state_arc.lock().unwrap();
        
        println!("\n--- Monitoring for Termination ---");
        println!(
            "CA ({}) weight: {:.4}, state: {:?}, consecutive idle checks: {}",
            controlling_agent_id, current_ca_weight, current_ca_state, consecutive_idle_checks
        );

        // Termination condition: CA has full weight (or very close) AND CA is idle.
        let weight_threshold = 0.001; // Allow small floating point errors
        let weight_recovered = (INITIAL_WEIGHT - current_ca_weight) < weight_threshold;
        let ca_is_idle = current_ca_state == ProcessState::Idle;
        
        if weight_recovered && ca_is_idle {
            consecutive_idle_checks += 1;
            println!("Termination condition partially met: weight recovered and CA idle (check {}/{})", 
                consecutive_idle_checks, MAX_IDLE_CHECKS);
                
            if consecutive_idle_checks >= MAX_IDLE_CHECKS {
                println!("\n--- TERMINATION DETECTED! ---");
                println!(
                    "CA ({}) final weight: {:.6}, state: {:?}",
                    controlling_agent_id, current_ca_weight, current_ca_state
                );
                println!("Weight difference from initial: {:.6}", (current_ca_weight - INITIAL_WEIGHT).abs());
                break; // Exit termination loop
            }
        } else {
            // Reset counter if conditions are not met
            consecutive_idle_checks = 0;
            if !weight_recovered {
                println!("Weight not fully recovered: {:.6} (need {:.6})", current_ca_weight, INITIAL_WEIGHT);
            }
            if !ca_is_idle {
                println!("CA is not idle: {:?}", current_ca_state);
            }
        }
    }

    println!("\n--- Simulation finished. Waiting for threads to join... ---");

    // Join all process threads to ensure they complete
    for handle in process_handles {
        let _ = handle.join();
    }
    // Join the dispatcher thread
    let _ = dispatcher_handle.join();

    println!("All threads joined. Exiting.");
}